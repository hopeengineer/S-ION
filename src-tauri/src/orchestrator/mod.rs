pub mod egress;
pub mod heartbeat;
pub mod router;
pub mod sandbox;
pub mod sentinel;
pub mod translator;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

/// Root structure for the SAM_LOGIC.yaml manifest.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SamLogic {
    pub version: String,
    pub engine_name: String,
    pub guardian_model: String,
    pub constitution: Constitution,
    pub engineering_standards: EngineeringStandards,
    pub privacy: PrivacyConfig,
    pub ux_logic: UxLogic,
    pub swarm: SwarmRoster,
    pub smart_mode: SmartModeConfig,
    pub expert_mode: ExpertModeConfig,
    pub audit_rules: AuditRules,
    pub copaw: CoPawConfig,
    pub safety: SafetyConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Constitution {
    pub zero_assumption_directive: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineeringStandards {
    pub html: String,
    pub css: String,
    pub logic: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrivacyConfig {
    pub network: String,
    pub storage: String,
    pub execution: String,
    #[serde(default)]
    pub egress_allowlist: Vec<String>,
    #[serde(default)]
    pub user_allowlist: Vec<String>,
    #[serde(default)]
    pub sentinel: SentinelConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SentinelConfig {
    #[serde(default)]
    pub railway_endpoint: String,
    #[serde(default)]
    pub developer_id: String,
    #[serde(default = "default_max_events")]
    pub max_local_events: usize,
    #[serde(default = "default_batch_interval")]
    pub batch_interval_secs: u64,
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            railway_endpoint: String::new(),
            developer_id: String::new(),
            max_local_events: 200,
            batch_interval_secs: 300,
        }
    }
}

fn default_max_events() -> usize {
    200
}
fn default_batch_interval() -> u64 {
    300
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UxLogic {
    pub jargon_filter: String,
    pub veto_power: String,
    pub theme: ThemeConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThemeConfig {
    pub background: String,
    pub accent: String,
}

// ──────────────────────────────────────────────────
// 8-Agent Swarm Roster
// ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmRoster {
    pub commander: SwarmAgent,
    pub audit_hook: SwarmAgent,
    pub analyst: SwarmAgent,
    pub visionary: SwarmAgent,
    pub builder: SwarmAgent,
    pub scout: SwarmAgent,
    pub fast_designer: SwarmAgent,
    pub pro_designer: SwarmAgent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmAgent {
    pub model: String,
    pub designation: String,
    pub role: String,
    pub api_env_key: String,
    pub api_base_url: String,
    pub triggers: Vec<String>,
}

// ──────────────────────────────────────────────────
// Smart Mode Config (Gemini Flash Triage)
// ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SmartModeConfig {
    pub triage_model: String,
    pub triage_api_env_key: String,
    pub triage_api_base_url: String,
    pub categories: Vec<TriageCategory>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TriageCategory {
    pub id: String,
    pub description: String,
    pub route_to: String,
}

// ──────────────────────────────────────────────────
// Expert Mode Config (Manual Pins)
// ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExpertModeConfig {
    pub default_pins: HashMap<String, String>,
}

// ──────────────────────────────────────────────────
// Audit Rules
// ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditRules {
    pub require_approval_for: Vec<String>,
    pub max_parallel_agents: u32,
    pub max_plan_steps: u32,
    pub reject_if: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoPawConfig {
    pub bridge_url: String,
    pub channels: Vec<String>,
    pub resync_on_wake: bool,
    pub queue_strategy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SafetyConfig {
    pub sandbox: String,
    pub snapshot_before_execution: bool,
    pub max_vm_lifetime_seconds: u64,
    pub auto_incinerate_on_error: bool,
}

// ──────────────────────────────────────────────────
// Execution Plan — Kimi K2.5 Commander output
// ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExecutionStep {
    pub step_id: u32,
    pub agent: String,
    pub action: String,
    pub tool_calls: Vec<String>,
    pub depends_on: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExecutionPlan {
    pub intent: String,
    pub steps: Vec<ExecutionStep>,
    pub reasoning: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditVerdict {
    pub approved: bool,
    pub reasoning: String,
    pub violations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PipelineResult {
    pub stage: String,
    pub plan: Option<ExecutionPlan>,
    pub verdict: Option<AuditVerdict>,
    pub error: Option<String>,
}

// ──────────────────────────────────────────────────
// Heuristic Router (fallback)
// ──────────────────────────────────────────────────

impl SamLogic {
    pub fn route_heuristic(&self, intent: &str) -> String {
        let intent_lower = intent.to_lowercase();

        let agents = [
            &self.swarm.commander,
            &self.swarm.audit_hook,
            &self.swarm.analyst,
            &self.swarm.visionary,
            &self.swarm.builder,
            &self.swarm.scout,
        ];

        for agent in &agents {
            for trigger in &agent.triggers {
                let keywords: Vec<&str> = trigger.split('_').collect();
                let match_count = keywords
                    .iter()
                    .filter(|kw| intent_lower.contains(**kw))
                    .count();

                if match_count > 0 && match_count >= (keywords.len() + 1) / 2 {
                    return serde_json::json!({
                        "stage": "fallback",
                        "model": agent.model,
                        "designation": agent.designation,
                    })
                    .to_string();
                }
            }
        }

        serde_json::json!({
            "stage": "fallback",
            "model": self.swarm.analyst.model,
            "designation": self.swarm.analyst.designation,
        })
        .to_string()
    }
}

// ──────────────────────────────────────────────────
// Stage 1: Kimi K2.5 Commander — Plan Decomposition
// ──────────────────────────────────────────────────

pub async fn call_kimi_commander(
    intent: &str,
    sam_logic: &SamLogic,
) -> Result<ExecutionPlan, String> {
    let api_key = env::var(&sam_logic.swarm.commander.api_env_key)
        .map_err(|_| format!("Missing env: {}", sam_logic.swarm.commander.api_env_key))?;

    if api_key.is_empty() {
        return Err("KIMI_API_KEY is empty".into());
    }

    let system_prompt = format!(
        r#"You are the S-ION Swarm Commander (Kimi K2.5). Decompose user intents into a parallel execution plan.

{constitution}

Available Sub-Agents:
- analyst ({analyst_model}): {analyst_role}
- visionary ({visionary_model}): {visionary_role}
- builder ({builder_model}): {builder_role}
- scout ({scout_model}): {scout_role}
- fast_designer ({fast_designer_model}): {fast_designer_role}
- pro_designer ({pro_designer_model}): {pro_designer_role}

Rules: Max {max_steps} steps, max {max_agents} parallel agents.

Respond ONLY with valid JSON:
{{"intent":"<intent>","steps":[{{"step_id":1,"agent":"<key>","action":"<desc>","tool_calls":[],"depends_on":[]}}],"reasoning":"<why>"}}"#,
        constitution = sam_logic.constitution.zero_assumption_directive,
        analyst_model = sam_logic.swarm.analyst.model,
        analyst_role = sam_logic.swarm.analyst.role,
        visionary_model = sam_logic.swarm.visionary.model,
        visionary_role = sam_logic.swarm.visionary.role,
        builder_model = sam_logic.swarm.builder.model,
        builder_role = sam_logic.swarm.builder.role,
        scout_model = sam_logic.swarm.scout.model,
        scout_role = sam_logic.swarm.scout.role,
        fast_designer_model = sam_logic.swarm.fast_designer.model,
        fast_designer_role = sam_logic.swarm.fast_designer.role,
        pro_designer_model = sam_logic.swarm.pro_designer.model,
        pro_designer_role = sam_logic.swarm.pro_designer.role,
        max_steps = sam_logic.audit_rules.max_plan_steps,
        max_agents = sam_logic.audit_rules.max_parallel_agents,
    );

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/chat/completions",
            sam_logic.swarm.commander.api_base_url
        ))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "kimi-k2-0201",
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": intent }
            ],
            "temperature": 0.3,
            "response_format": { "type": "json_object" }
        }))
        .send()
        .await
        .map_err(|e| format!("Kimi API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Kimi response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Kimi API error ({}): {}", status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON from Kimi: {}", e))?;

    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "No content in Kimi response".to_string())?;

    let plan: ExecutionPlan =
        serde_json::from_str(content).map_err(|e| format!("Failed to parse Kimi plan: {}", e))?;

    println!("🎯 Commander Plan: {} steps", plan.steps.len());
    Ok(plan)
}

// ──────────────────────────────────────────────────
// Stage 2: Opus 4.6 Audit Hook
// ──────────────────────────────────────────────────

pub async fn call_opus_audit(
    plan: &ExecutionPlan,
    sam_logic: &SamLogic,
) -> Result<AuditVerdict, String> {
    let api_key = env::var(&sam_logic.swarm.audit_hook.api_env_key)
        .map_err(|_| format!("Missing env: {}", sam_logic.swarm.audit_hook.api_env_key))?;

    if api_key.is_empty() {
        return Err("ANTHROPIC_API_KEY is empty".into());
    }

    let plan_json = serde_json::to_string_pretty(plan)
        .map_err(|e| format!("Failed to serialize plan: {}", e))?;
    let audit_rules_json = serde_json::to_string_pretty(&sam_logic.audit_rules)
        .map_err(|e| format!("Failed to serialize rules: {}", e))?;

    let system_prompt = format!(
        r#"You are the S-ION Audit Hook (Opus 4.6). Review the execution plan and APPROVE or REJECT.

Audit Rules:
{audit_rules}

Sandbox: {sandbox}, Max VM: {max_vm}s

Respond ONLY with JSON: {{"approved":true/false,"reasoning":"<why>","violations":[]}}"#,
        audit_rules = audit_rules_json,
        sandbox = sam_logic.safety.sandbox,
        max_vm = sam_logic.safety.max_vm_lifetime_seconds,
    );

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/messages",
            sam_logic.swarm.audit_hook.api_base_url
        ))
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-opus-4-20250514",
            "max_tokens": 1024,
            "system": system_prompt,
            "messages": [{ "role": "user", "content": format!("Review:\n\n{}", plan_json) }]
        }))
        .send()
        .await
        .map_err(|e| format!("Opus API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Opus response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Opus API error ({}): {}", status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON from Opus: {}", e))?;

    let content = resp_json["content"][0]["text"]
        .as_str()
        .ok_or_else(|| "No content in Opus response".to_string())?;

    let json_str = extract_json(content);
    let verdict: AuditVerdict = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse verdict: {} — raw: {}", e, content))?;

    let emoji = if verdict.approved { "✅" } else { "🚫" };
    println!("{} Audit: {}", emoji, verdict.reasoning);
    Ok(verdict)
}

// ──────────────────────────────────────────────────
// Two-Stage Pipeline
// ──────────────────────────────────────────────────

pub async fn route_intent_live(intent: &str, sam_logic: &SamLogic) -> PipelineResult {
    let plan = match call_kimi_commander(intent, sam_logic).await {
        Ok(p) => p,
        Err(e) => {
            return PipelineResult {
                stage: "commander_error".into(),
                plan: None,
                verdict: None,
                error: Some(e),
            };
        }
    };

    let verdict = match call_opus_audit(&plan, sam_logic).await {
        Ok(v) => v,
        Err(e) => {
            return PipelineResult {
                stage: "audit_error".into(),
                plan: Some(plan),
                verdict: None,
                error: Some(e),
            };
        }
    };

    PipelineResult {
        stage: if verdict.approved {
            "approved"
        } else {
            "rejected"
        }
        .into(),
        plan: Some(plan),
        verdict: Some(verdict),
        error: None,
    }
}

// ──────────────────────────────────────────────────
// Utility: Extract JSON from LLM response
// ──────────────────────────────────────────────────

pub fn extract_json(content: &str) -> String {
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = content.find('{') {
        let mut depth = 0;
        let chars: Vec<char> = content[start..].chars().collect();
        for (i, ch) in chars.iter().enumerate() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return content[start..start + i + 1].to_string();
                    }
                }
                _ => {}
            }
        }
    }
    content.trim().to_string()
}
