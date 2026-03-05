use crate::orchestrator::{extract_json, SamLogic};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

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
                "maxOutputTokens": 512
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

    let content = resp_json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "No content in Gemini triage response. Full response: {}",
                body
            )
        })?;

    // Use extract_json to handle any remaining preamble
    let json_str = extract_json(content);

    println!("🔎 Gemini raw triage: {}", json_str);

    let triage: TriageResult = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse triage result: {} — raw: {}", e, content))?;

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
            "model": "deepseek-chat",
            "messages": [
                {
                    "role": "system",
                    "content": "You are S-ION's Analyst (DeepSeek V3). Provide clear, concise, high-quality answers."
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

/// Dispatches an intent in Smart Mode: triage with Gemini Flash → route to best model.
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
    let (agent_key, model_name, designation) = resolve_agent(&triage.route_to, sam_logic);

    DispatchResult {
        mode: "smart".into(),
        triage: Some(triage),
        routed_to: agent_key,
        model_name,
        designation,
        response: None,
        error: None,
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

    let (resolved_key, model_name, designation) = resolve_agent(&agent_key, sam_logic);

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
fn resolve_agent(agent_key: &str, sam_logic: &SamLogic) -> (String, String, String) {
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
        _ => (
            "analyst".into(),
            sam_logic.swarm.analyst.model.clone(),
            sam_logic.swarm.analyst.designation.clone(),
        ),
    }
}
