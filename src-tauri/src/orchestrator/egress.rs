use crate::orchestrator::SamLogic;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ──────────────────────────────────────────────────
// Security Event (logged for every network request)
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SecurityEvent {
    pub timestamp: String,
    pub domain: String,
    pub full_url: String,
    pub status: String, // "pass" or "blocked"
    pub agent_key: String,
}

// ──────────────────────────────────────────────────
// Egress Filter: Domain Allowlist Gate
// ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EgressFilter {
    pub allowlist: Vec<String>,
    pub log: Arc<Mutex<VecDeque<SecurityEvent>>>,
    pub max_events: usize,
}

impl EgressFilter {
    /// Builds the EgressFilter from SAM_LOGIC.yaml privacy config.
    pub fn from_sam_logic(sam_logic: &SamLogic) -> Self {
        let mut allowlist = sam_logic.privacy.egress_allowlist.clone();
        allowlist.extend(sam_logic.privacy.user_allowlist.clone());
        let max_events = sam_logic.privacy.sentinel.max_local_events;

        println!(
            "🛡️  Egress Filter initialized with {} allowed domains",
            allowlist.len()
        );
        for domain in &allowlist {
            println!("   ✅ {}", domain);
        }

        Self {
            allowlist,
            log: Arc::new(Mutex::new(VecDeque::with_capacity(max_events))),
            max_events,
        }
    }

    /// Validates a URL against the egress allowlist.
    /// Returns Ok(()) if allowed, Err(reason) if blocked.
    pub fn validate(&self, url: &str, agent_key: &str) -> Result<(), String> {
        let domain = extract_domain(url);
        let now = chrono::Utc::now().to_rfc3339();

        let is_allowed = self.allowlist.iter().any(|d| domain.contains(d));

        let event = SecurityEvent {
            timestamp: now,
            domain: domain.clone(),
            full_url: url.to_string(),
            status: if is_allowed {
                "pass".into()
            } else {
                "blocked".into()
            },
            agent_key: agent_key.to_string(),
        };

        // Log the event
        if let Ok(mut log) = self.log.lock() {
            if log.len() >= self.max_events {
                log.pop_front();
            }
            log.push_back(event.clone());
        }

        if is_allowed {
            println!("✅ Egress PASS: {} → {}", agent_key, domain);
            Ok(())
        } else {
            println!("🚫 Egress BLOCKED: {} → {}", agent_key, domain);
            Err(format!(
                "Egress blocked: domain '{}' is not in the allowlist",
                domain
            ))
        }
    }

    /// Returns the current security log as a JSON-serializable vector.
    pub fn get_log(&self) -> Vec<SecurityEvent> {
        self.log
            .lock()
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Adds a user-defined domain to the allowlist at runtime.
    pub fn add_user_domain(&mut self, domain: &str) {
        if !self.allowlist.contains(&domain.to_string()) {
            self.allowlist.push(domain.to_string());
            println!("🛡️  User allowlist updated: added {}", domain);
        }
    }
}

/// Extracts the domain from a URL string.
fn extract_domain(url: &str) -> String {
    url.replace("https://", "")
        .replace("http://", "")
        .split('/')
        .next()
        .unwrap_or("unknown")
        .split(':')
        .next()
        .unwrap_or("unknown")
        .to_string()
}
