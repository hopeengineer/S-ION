use crate::orchestrator::SamLogic;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ──────────────────────────────────────────────────
// Sentinel Report: The PII-scrubbed crash report
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SentinelReport {
    pub install_id: String,
    pub app_version: String,
    pub event_type: String,  // "model_error", "panic", "egress_blocked"
    pub error_code: String,  // e.g. "LLM_INVALID_TOOL", "HTTP_401"
    pub logic_trace: String, // e.g. "Triage_Flash -> Route_DeepSeek -> 401_Error"
    pub model_used: String,
    pub agent_key: String,
    pub blocked_domain: Option<String>,
    pub timestamp: String,
}

// ──────────────────────────────────────────────────
// Sentinel: Local Error Buffer + Privacy Scrubber
// ──────────────────────────────────────────────────

pub struct Sentinel {
    pub install_id: String,
    pub app_version: String,
    pub railway_endpoint: String,
    pub developer_id: String,
    pub pending_reports: Arc<Mutex<VecDeque<SentinelReport>>>,
    pub max_pending: usize,
}

impl Sentinel {
    /// Initialize Sentinel from SAM_LOGIC config.
    pub fn new(sam_logic: &SamLogic) -> Self {
        let install_id = load_or_create_install_id();
        println!("🔭 Sentinel initialized (install: {})", &install_id[..8]);

        Self {
            install_id,
            app_version: sam_logic.version.clone(),
            railway_endpoint: sam_logic.privacy.sentinel.railway_endpoint.clone(),
            developer_id: sam_logic.privacy.sentinel.developer_id.clone(),
            pending_reports: Arc::new(Mutex::new(VecDeque::with_capacity(50))),
            max_pending: 50,
        }
    }

    /// Capture an error event: scrubs PII, builds a SentinelReport, and queues it.
    pub fn capture_error(
        &self,
        event_type: &str,
        error_code: &str,
        raw_error: &str,
        model_used: &str,
        agent_key: &str,
        blocked_domain: Option<&str>,
    ) -> SentinelReport {
        let logic_trace = scrub_pii(raw_error);
        let now = chrono::Utc::now().to_rfc3339();

        let report = SentinelReport {
            install_id: self.install_id.clone(),
            app_version: self.app_version.clone(),
            event_type: event_type.to_string(),
            error_code: error_code.to_string(),
            logic_trace,
            model_used: model_used.to_string(),
            agent_key: agent_key.to_string(),
            blocked_domain: blocked_domain.map(|s| s.to_string()),
            timestamp: now,
        };

        // Queue the report for user consent
        if let Ok(mut pending) = self.pending_reports.lock() {
            if pending.len() >= self.max_pending {
                pending.pop_front();
            }
            pending.push_back(report.clone());
        }

        println!(
            "🔭 Sentinel captured: [{}] {} -> {}",
            report.event_type, report.error_code, report.agent_key
        );
        report
    }

    /// Get the oldest pending report (for consent toast).
    pub fn get_pending_report(&self) -> Option<SentinelReport> {
        self.pending_reports
            .lock()
            .ok()
            .and_then(|q| q.front().cloned())
    }

    /// User approved: send the oldest pending report to Railway.
    pub async fn approve_and_send(&self) -> Result<String, String> {
        let report = {
            let mut pending = self.pending_reports.lock().map_err(|e| e.to_string())?;
            pending.pop_front()
        };

        match report {
            Some(r) => {
                if self.railway_endpoint.is_empty() {
                    println!(
                        "🔭 Sentinel: No Railway endpoint configured, report saved locally only."
                    );
                    return Ok("Report saved locally (no endpoint configured)".into());
                }
                send_to_railway(&self.railway_endpoint, &r).await
            }
            None => Ok("No pending reports".into()),
        }
    }

    /// User dismissed: discard the oldest pending report.
    pub fn dismiss_report(&self) {
        if let Ok(mut pending) = self.pending_reports.lock() {
            pending.pop_front();
        }
    }

    /// Check if the current user is the developer/founder.
    pub fn is_founder(&self) -> bool {
        !self.developer_id.is_empty() && self.install_id == self.developer_id
    }
}

// ──────────────────────────────────────────────────
// Triple-Pass Privacy Scrubber
// ──────────────────────────────────────────────────

/// Aggressively scrubs PII from error strings using three passes:
/// 1. Regex Pass: emails, IPs, file paths
/// 2. Structural Pass: prompt/completion content
/// 3. Entropy Pass: API key-like strings
pub fn scrub_pii(raw: &str) -> String {
    let pass1 = regex_pass(raw);
    let pass2 = structural_pass(&pass1);
    let pass3 = entropy_pass(&pass2);
    pass3
}

/// Pass 1: Strip emails, IPs, Mac/Windows file paths, and usernames.
fn regex_pass(input: &str) -> String {
    let mut result = input.to_string();

    // Email addresses
    let email_re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    result = email_re
        .replace_all(&result, "[EMAIL_REDACTED]")
        .to_string();

    // IPv4 addresses
    let ip_re = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
    result = ip_re.replace_all(&result, "[IP_REDACTED]").to_string();

    // Mac file paths: /Users/xxx/... or /home/xxx/...
    let mac_path_re = Regex::new(r#"(/Users/|/home/)[^\s\x22\x27]+"#).unwrap();
    result = mac_path_re
        .replace_all(&result, "[PATH_REDACTED]")
        .to_string();

    // Windows file paths: C:\Users\xxx\...
    let win_path_re = Regex::new(r#"[A-Z]:\\[^\s\x22\x27]+"#).unwrap();
    result = win_path_re
        .replace_all(&result, "[PATH_REDACTED]")
        .to_string();

    // URLs with usernames/tokens in path
    let url_token_re = Regex::new(r"https?://[^\s]+/[^\s]*token[^\s]*").unwrap();
    result = url_token_re
        .replace_all(&result, "[URL_REDACTED]")
        .to_string();

    result
}

/// Pass 2: Strip any content inside "prompt", "completion", "content", "message" JSON fields.
fn structural_pass(input: &str) -> String {
    let mut result = input.to_string();

    // Match JSON-like "prompt": "..." or "content": "..." patterns
    let fields = [
        "prompt",
        "completion",
        "content",
        "message",
        "text",
        "query",
    ];
    for field in fields {
        let pattern = format!(r#""{}":\s*"[^"]*""#, field);
        if let Ok(re) = Regex::new(&pattern) {
            let replacement = format!(r#""{}": "[STRIPPED_CONTENT]""#, field);
            result = re.replace_all(&result, replacement.as_str()).to_string();
        }
    }

    result
}

/// Pass 3: Redact high-entropy strings that look like API keys.
/// Matches strings of 20+ alphanumeric characters with mixed case/digits.
fn entropy_pass(input: &str) -> String {
    let key_re = Regex::new(r"\b[A-Za-z0-9_-]{32,}\b").unwrap();
    key_re
        .replace_all(input, |caps: &regex::Captures| {
            let matched = caps.get(0).unwrap().as_str();
            // Only redact if it has mixed character types (likely a key)
            let has_upper = matched.chars().any(|c| c.is_uppercase());
            let has_lower = matched.chars().any(|c| c.is_lowercase());
            let has_digit = matched.chars().any(|c| c.is_ascii_digit());
            if (has_upper && has_lower) || (has_upper && has_digit) || (has_lower && has_digit) {
                "[API_KEY_REDACTED]".to_string()
            } else {
                matched.to_string()
            }
        })
        .to_string()
}

// ──────────────────────────────────────────────────
// Utilities
// ──────────────────────────────────────────────────

/// Load or create a persistent anonymous install ID.
fn load_or_create_install_id() -> String {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("s-ion");
    let id_path = config_dir.join("install_id");

    if let Ok(id) = std::fs::read_to_string(&id_path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::create_dir_all(&config_dir);
    let _ = std::fs::write(&id_path, &new_id);
    new_id
}

/// Send a scrubbed report to the Railway /telemetry endpoint.
async fn send_to_railway(endpoint: &str, report: &SentinelReport) -> Result<String, String> {
    let token = std::env::var("SION_BRIDGE_TOKEN").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut req = client
        .post(format!("{}/telemetry", endpoint))
        .header("Content-Type", "application/json")
        .json(report);

    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let res = req
        .send()
        .await
        .map_err(|e| format!("Sentinel send failed: {}", e))?;

    if res.status().is_success() {
        println!("🔭 Sentinel: Report sent successfully");
        Ok("Report sent".into())
    } else {
        let status = res.status().as_u16();
        Err(format!("Sentinel endpoint returned {}", status))
    }
}
