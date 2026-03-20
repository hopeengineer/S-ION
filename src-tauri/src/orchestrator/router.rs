use crate::orchestrator::{extract_json, SamLogic};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

/// Extract a JSON field value from potentially truncated JSON.
/// Looks for `"field":"value"` or `"field": "value"` pattern.
fn extract_field(content: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\"", field);
    if let Some(pos) = content.find(&pattern) {
        let after = &content[pos + pattern.len()..];
        // Skip `: ` or `:`
        let after = after.trim_start().strip_prefix(':')?.trim_start();
        if after.starts_with('"') {
            let value_start = 1;
            if let Some(end) = after[value_start..].find('"') {
                return Some(after[value_start..value_start + end].to_string());
            }
        }
    }
    None
}

// ──────────────────────────────────────────────────
// Runtime Mode
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeMode {
    Smart,
    Expert,
}

// ──────────────────────────────────────────────────
// Triage Result (Smart Mode)
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    pub category: String, // e.g. "simple_qa", "parallel_ui", "deep_code", "long_context"
    pub route_to: String, // agent key: "analyst", "commander", "builder", "visionary"
    pub reasoning: String,
    pub confidence: f64,
}

// ──────────────────────────────────────────────────
// Dispatch Result
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchResult {
    pub mode: String, // "smart" or "expert"
    pub triage: Option<TriageResult>,
    pub routed_to: String,   // agent key
    pub model_name: String,  // actual model name
    pub designation: String, // human-readable designation
    pub response: Option<String>,
    pub error: Option<String>,
}

// ──────────────────────────────────────────────────
// Expert Mode Pins
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertPins {
    pub pins: HashMap<String, String>, // task_category → agent_key
}

impl ExpertPins {
    pub fn from_yaml_defaults(defaults: &HashMap<String, String>) -> Self {
        Self {
            pins: defaults.clone(),
        }
    }

    pub fn set_pin(&mut self, category: &str, agent_key: &str) {
        self.pins
            .insert(category.to_string(), agent_key.to_string());
    }

    pub fn get_pin(&self, category: &str) -> Option<&String> {
        self.pins.get(category)
    }
}

// ──────────────────────────────────────────────────
// Smart Mode: Gemini Flash Triage
// ──────────────────────────────────────────────────

/// Calls Gemini Flash to classify an intent into one of the triage categories.
pub async fn call_gemini_flash_triage(
    intent: &str,
    sam_logic: &SamLogic,
) -> Result<TriageResult, String> {
    let api_key = env::var(&sam_logic.smart_mode.triage_api_env_key)
        .map_err(|_| format!("Missing env: {}", sam_logic.smart_mode.triage_api_env_key))?;

    if api_key.is_empty() {
        return Err("GEMINI_API_KEY is empty".into());
    }

    let categories_desc: Vec<String> = sam_logic
        .smart_mode
        .categories
        .iter()
        .map(|c| format!("- {}: {} → routes to {}", c.id, c.description, c.route_to))
        .collect();

    let system_instruction = format!(
        "You are a JSON-only intent classifier. You must respond with ONLY a raw JSON object, nothing else.\n\
        Classify the user's intent into exactly one category.\n\
        Categories:\n{}\n\n\
        Your entire response must be exactly one JSON object: \
        {{\"category\":\"<id>\",\"route_to\":\"<agent>\",\"reasoning\":\"<why>\",\"confidence\":<0.0-1.0>}}",
        categories_desc.join("\n")
    );

    let url = format!(
        "{}/models/{}:generateContent?key={}",
        sam_logic.smart_mode.triage_api_base_url, sam_logic.smart_mode.triage_model, api_key
    );

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "system_instruction": {
                "parts": [{ "text": system_instruction }]
            },
            "contents": [
                {
                    "parts": [
                        { "text": intent }
                    ]
                }
            ],
            "generationConfig": {
                "temperature": 0.05,
                "maxOutputTokens": 512,
                "responseMimeType": "application/json"
            }
        }))
        .send()
        .await
        .map_err(|e| format!("Gemini Flash triage request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Gemini response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Gemini Flash triage error ({}): {}", status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON from Gemini: {}", e))?;

    // Diagnostic: log finish reason
    let finish_reason = resp_json["candidates"][0]["finishReason"]
        .as_str()
        .unwrap_or("unknown");
    println!("🔎 Gemini triage finishReason: {}", finish_reason);
    println!("🔎 Gemini full body: {}", &body[..body.len().min(500)]);

    let content = resp_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "No content in Gemini triage response. Full response: {}",
                body
            )
        })?;

    println!("🔎 Gemini raw content: {}", content);

    // Use extract_json to handle any remaining preamble
    let json_str = extract_json(content);

    println!("🔎 Gemini extracted JSON: {}", json_str);

    // Try to parse, if it fails and looks like truncated triage, try to recover
    let triage: TriageResult = match serde_json::from_str(&json_str) {
        Ok(t) => t,
        Err(e) => {
            // Attempt recovery: if we can detect category from the truncated JSON
            println!("⚠️  Triage parse failed: {} — attempting recovery from: {}", e, content);

            // Try to extract category and route_to from partial JSON
            let cat = extract_field(content, "category");
            let route = extract_field(content, "route_to");

            if let (Some(category), Some(route_to)) = (cat, route) {
                println!("🔧 Recovered triage: {} → {}", category, route_to);
                TriageResult {
                    category,
                    route_to,
                    reasoning: format!("Recovered from truncated triage: {}", content),
                    confidence: 0.7,
                }
            } else {
                return Err(format!("Failed to parse triage result: {} — raw: {}", e, content));
            }
        }
    };

    println!(
        "⚡ Triage: {} → {} (confidence: {:.0}%)",
        triage.category,
        triage.route_to,
        triage.confidence * 100.0
    );

    Ok(triage)
}

// ──────────────────────────────────────────────────
// DeepSeek V3 API Client (OpenAI-compatible)
// ──────────────────────────────────────────────────

/// Calls DeepSeek V3 for low-cost logic tasks.
pub async fn call_deepseek(intent: &str, sam_logic: &SamLogic) -> Result<String, String> {
    let api_key = env::var(&sam_logic.swarm.analyst.api_env_key)
        .map_err(|_| format!("Missing env: {}", sam_logic.swarm.analyst.api_env_key))?;

    if api_key.is_empty() {
        return Err("DEEPSEEK_API_KEY is empty".into());
    }

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/chat/completions",
            sam_logic.swarm.analyst.api_base_url
        ))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": sam_logic.swarm.analyst.model,
            "messages": [
                {
                    "role": "system",
                    "content": format!("You are S-ION's Analyst ({}). Provide clear, concise, high-quality answers.\n{}", sam_logic.swarm.analyst.model, sam_logic.constitution.zero_assumption_directive)
                },
                { "role": "user", "content": intent }
            ],
            "temperature": 0.3,
            "max_tokens": 2048
        }))
        .send()
        .await
        .map_err(|e| format!("DeepSeek API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read DeepSeek response: {}", e))?;

    if !status.is_success() {
        return Err(format!("DeepSeek API error ({}): {}", status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON from DeepSeek: {}", e))?;

    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "No content in DeepSeek response".to_string())?;

    println!(
        "🔍 Analyst (DeepSeek V3) responded: {}...",
        &content[..content.len().min(80)]
    );

    Ok(content.to_string())
}

// ──────────────────────────────────────────────────
// Generic OpenAI-compatible API Client
// ──────────────────────────────────────────────────

/// Calls any OpenAI-compatible API (GPT, DeepSeek, Kimi).
pub async fn call_openai_compatible(
    intent: &str,
    api_key: &str,
    base_url: &str,
    model: &str,
    system_prompt: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": intent }
            ],
            "temperature": 0.3,
            "max_tokens": 2048
        }))
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("API error ({}): {}", status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {}", e))?;

    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "No content in response".to_string())?;

    Ok(content.to_string())
}

// ──────────────────────────────────────────────────
// Smart Mode Dispatcher
// ──────────────────────────────────────────────────

/// Dispatches an intent in Smart Mode: triage with Gemini Flash → route to best model → call it.
pub async fn dispatch_smart(intent: &str, sam_logic: &SamLogic) -> DispatchResult {
    // Step 1: Triage with Gemini Flash
    let triage = match call_gemini_flash_triage(intent, sam_logic).await {
        Ok(t) => t,
        Err(e) => {
            println!("⚠️  Smart triage failed: {}. Defaulting to analyst.", e);
            TriageResult {
                category: "simple_qa".into(),
                route_to: "analyst".into(),
                reasoning: format!("Triage fallback: {}", e),
                confidence: 0.0,
            }
        }
    };

    // Step 2: Resolve agent from triage result
    let (agent_key, model_name, designation) = resolve_agent_public(&triage.route_to, sam_logic);

    // Step 3: Call the routed agent's API
    let (response, error) = match agent_key.as_str() {
        "analyst" => {
            // DeepSeek V3 via dedicated client
            match call_deepseek(intent, sam_logic).await {
                Ok(r) => (Some(r), None),
                Err(e) => (None, Some(e)),
            }
        }
        "commander" | "scout" => {
            // OpenAI-compatible APIs (Kimi / GPT)
            let agent = match agent_key.as_str() {
                "commander" => &sam_logic.swarm.commander,
                _ => &sam_logic.swarm.scout,
            };
            let api_key = env::var(&agent.api_env_key).unwrap_or_default();
            if api_key.is_empty() {
                (None, Some(format!("Missing env: {}", agent.api_env_key)))
            } else {
                match call_openai_compatible(
                    intent,
                    &api_key,
                    &agent.api_base_url,
                    &agent.model,
                    &format!(
                        "You are S-ION's {} ({}). {}\n{}",
                        agent.designation,
                        agent.model,
                        agent.role,
                        sam_logic.constitution.zero_assumption_directive
                    ),
                )
                .await
                {
                    Ok(r) => (Some(r), None),
                    Err(e) => (None, Some(e)),
                }
            }
        }
        _ => {
            // For agents without direct API (visionary, builder, etc.),
            // fall back to DeepSeek as a cost-effective default
            match call_deepseek(intent, sam_logic).await {
                Ok(r) => (Some(r), None),
                Err(e) => (None, Some(e)),
            }
        }
    };

    DispatchResult {
        mode: "smart".into(),
        triage: Some(triage),
        routed_to: agent_key,
        model_name,
        designation,
        response,
        error,
    }
}

/// Dispatches an intent in Expert Mode: use the pinned model for the task category.
pub fn dispatch_expert(
    _intent: &str,
    task_category: &str,
    pins: &ExpertPins,
    sam_logic: &SamLogic,
) -> DispatchResult {
    let agent_key = pins
        .get_pin(task_category)
        .cloned()
        .unwrap_or_else(|| "analyst".to_string());

    let (resolved_key, model_name, designation) = resolve_agent_public(&agent_key, sam_logic);

    DispatchResult {
        mode: "expert".into(),
        triage: None,
        routed_to: resolved_key,
        model_name,
        designation,
        response: None,
        error: None,
    }
}

/// Resolves an agent key to its model name and designation.
pub fn resolve_agent_public(agent_key: &str, sam_logic: &SamLogic) -> (String, String, String) {
    match agent_key {
        "commander" => (
            "commander".into(),
            sam_logic.swarm.commander.model.clone(),
            sam_logic.swarm.commander.designation.clone(),
        ),
        "audit_hook" => (
            "audit_hook".into(),
            sam_logic.swarm.audit_hook.model.clone(),
            sam_logic.swarm.audit_hook.designation.clone(),
        ),
        "analyst" => (
            "analyst".into(),
            sam_logic.swarm.analyst.model.clone(),
            sam_logic.swarm.analyst.designation.clone(),
        ),
        "visionary" => (
            "visionary".into(),
            sam_logic.swarm.visionary.model.clone(),
            sam_logic.swarm.visionary.designation.clone(),
        ),
        "builder" => (
            "builder".into(),
            sam_logic.swarm.builder.model.clone(),
            sam_logic.swarm.builder.designation.clone(),
        ),
        "scout" => (
            "scout".into(),
            sam_logic.swarm.scout.model.clone(),
            sam_logic.swarm.scout.designation.clone(),
        ),
        "fast_designer" => (
            "fast_designer".into(),
            sam_logic.swarm.fast_designer.model.clone(),
            sam_logic.swarm.fast_designer.designation.clone(),
        ),
        "pro_designer" => (
            "pro_designer".into(),
            sam_logic.swarm.pro_designer.model.clone(),
            sam_logic.swarm.pro_designer.designation.clone(),
        ),
        _ => (
            "analyst".into(),
            sam_logic.swarm.analyst.model.clone(),
            sam_logic.swarm.analyst.designation.clone(),
        ),
    }
}

// ──────────────────────────────────────────────────
// Phase 7: ActionEnvelope (Strict LLM → Sandbox Contract)
// ──────────────────────────────────────────────────

/// The strict machine-to-machine JSON contract that LLMs must produce
/// when the triage routes an intent to the Action track (deep_code, parallel_ui).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub mission_id: String,
    /// Human-readable explanation for the UI (shown above the Action Card)
    pub explanation: String,
    /// Shell commands to execute in the sandbox (one per array element)
    pub bash_commands: Vec<String>,
    /// Files the agent expects to create or modify
    pub target_files: Vec<String>,
}

/// The result of the full orchestration loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationResult {
    /// "knowledge" or "action"
    pub track: String,
    /// Triage metadata
    pub triage: Option<TriageResult>,
    /// Which model handled this
    pub model_name: String,
    /// Text response (Knowledge track)
    pub response: Option<String>,
    /// Parsed ActionEnvelope (Action track, pre-sandbox)
    pub envelope: Option<ActionEnvelope>,
    /// Sandbox execution result (Action track, post-sandbox)
    pub sandbox_result: Option<serde_json::Value>,
    /// Error if any stage failed
    pub error: Option<String>,
}

// ──────────────────────────────────────────────────
// Deterministic Audit Hook (0ms, $0.00)
// ──────────────────────────────────────────────────

/// Dangerous command patterns that must NEVER reach the sandbox.
/// The Guest Agent has its own blocklist, but this is the host-side
/// pre-screen — defense in depth.
const BLOCKED_PATTERNS: &[&str] = &[
    // Network exfiltration
    "curl ", "curl\t", "wget ", "wget\t",
    "nc ", "nc\t", "ncat ", "netcat ",
    "ssh ", "scp ", "sftp ", "rsync ",
    "telnet ",
    // Reverse shells
    "/dev/tcp/", "/dev/udp/",
    "bash -i", "sh -i",
    // Package manager abuse (network required)
    "pip install", "pip3 install",
    "npm install", "npx ",
    "cargo install",
    "apt install", "apt-get install",
    "brew install", "yum install",
    // Destructive host operations
    "rm -rf /", "rm -rf /*",
    "mkfs", "dd if=",
    ":(){ :|:& };:",
    // Privilege escalation
    "sudo ", "su ",
    "chmod 777", "chown root",
    // Path traversal
    "../../../",
];

/// Validates every bash_command in the envelope against the blocklist.
/// Returns Ok(()) if safe, Err(reason) if blocked.
pub fn audit_envelope(envelope: &ActionEnvelope) -> Result<(), String> {
    for (i, cmd) in envelope.bash_commands.iter().enumerate() {
        let lower = cmd.to_lowercase();
        for pattern in BLOCKED_PATTERNS {
            if lower.contains(&pattern.to_lowercase()) {
                return Err(format!(
                    "BLOCKED: Command #{} contains dangerous pattern '{}' — full command: {}",
                    i + 1,
                    pattern,
                    cmd
                ));
            }
        }

        // Also block empty commands
        if cmd.trim().is_empty() {
            return Err(format!("BLOCKED: Command #{} is empty", i + 1));
        }
    }

    // Validate target_files don't contain path traversal
    for file in &envelope.target_files {
        if file.contains("../") || file.starts_with('/') || file.contains("\\..\\") {
            return Err(format!(
                "BLOCKED: target_file '{}' contains path traversal or absolute path",
                file
            ));
        }
    }

    println!(
        "✅ Audit passed: {} commands, {} target files",
        envelope.bash_commands.len(),
        envelope.target_files.len()
    );
    Ok(())
}

// ──────────────────────────────────────────────────
// Action Track: Force LLM into ActionEnvelope JSON
// ──────────────────────────────────────────────────

/// System prompt injected when calling LLMs in Action mode.
/// Forces structured JSON output matching ActionEnvelope.
const ACTION_SYSTEM_PROMPT: &str = r#"You are S-ION's Code Executor. You MUST respond with ONLY a raw JSON object matching this exact schema:

{
  "mission_id": "<unique-id>",
  "explanation": "<1-2 sentence explanation of what the code will do>",
  "bash_commands": ["<shell command 1>", "<shell command 2>"],
  "target_files": ["<file1.ext>", "<file2.ext>"]
}

Rules:
- bash_commands: Shell commands to run in an isolated sandbox. Each command runs sequentially via /bin/sh.
- target_files: Files that will be created or modified by the commands.
- Do NOT include markdown, backticks, or any text outside the JSON object.
- Do NOT use network commands (curl, wget, npm install). The sandbox has no network.
- Write files using echo/cat/heredoc syntax inside bash_commands.
- Keep commands simple and deterministic."#;

/// Calls the Commander (Kimi K2.5) in Action mode, forcing JSON output matching ActionEnvelope.
/// Always uses the Commander regardless of which agent triage selected, since the Commander
/// is the only agent configured for structured ActionEnvelope generation.
pub async fn dispatch_action(
    intent: &str,
    _agent_key: &str,
    sam_logic: &SamLogic,
) -> Result<ActionEnvelope, String> {
    // Always use Commander for ActionEnvelope generation
    let (api_key_env, base_url, model) = (
        sam_logic.swarm.commander.api_env_key.clone(),
        sam_logic.swarm.commander.api_base_url.clone(),
        sam_logic.swarm.commander.model.clone(),
    );

    println!("📦 dispatch_action: using Commander ({}) for ActionEnvelope", model);

    let api_key = env::var(&api_key_env)
        .map_err(|_| format!("Missing env: {}", api_key_env))?;

    if api_key.is_empty() {
        return Err(format!("{} is empty", api_key_env));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": ACTION_SYSTEM_PROMPT },
                { "role": "user", "content": intent }
            ],
            "temperature": 1.0,
            "max_tokens": 4096,
            "response_format": { "type": "json_object" }
        }))
        .send()
        .await
        .map_err(|e| format!("Action dispatch API failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Action dispatch API error ({}): {}", status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON response: {}", e))?;

    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "No content in Action response".to_string())?;

    // Extract JSON from potential markdown wrapping
    let json_str = crate::orchestrator::extract_json(content);

    println!(
        "📦 ActionEnvelope raw: {}",
        &json_str[..json_str.len().min(200)]
    );

    let envelope: ActionEnvelope = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse ActionEnvelope: {} — raw: {}", e, json_str))?;

    Ok(envelope)
}

