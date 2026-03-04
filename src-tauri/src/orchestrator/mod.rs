use serde::{Deserialize, Serialize};

/// Root structure for the SAM_LOGIC.yaml manifest.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SamLogic {
    pub version: String,
    pub engine_name: String,
    pub guardian_model: String,
    pub engineering_standards: EngineeringStandards,
    pub privacy: PrivacyConfig,
    pub ux_logic: UxLogic,
    pub swarm: SwarmRoster,
    pub copaw: CoPawConfig,
    pub safety: SafetyConfig,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmRoster {
    pub conductor: SwarmAgent,
    pub visionary: SwarmAgent,
    pub builder: SwarmAgent,
    pub scout: SwarmAgent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwarmAgent {
    pub model: String,
    pub designation: String,
    pub role: String,
    pub triggers: Vec<String>,
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

impl SamLogic {
    /// Routes a user intent string to the correct swarm model designation.
    /// Uses keyword matching against each agent's trigger list.
    pub fn route(&self, intent: &str) -> String {
        let intent_lower = intent.to_lowercase();

        // Check each agent's triggers for a keyword match
        let agents = [
            &self.swarm.conductor,
            &self.swarm.visionary,
            &self.swarm.builder,
            &self.swarm.scout,
        ];

        for agent in &agents {
            for trigger in &agent.triggers {
                // Split trigger on underscores for fuzzy matching
                let keywords: Vec<&str> = trigger.split('_').collect();
                let match_count = keywords
                    .iter()
                    .filter(|kw| intent_lower.contains(**kw))
                    .count();

                // If more than half the keywords match, route to this agent
                if match_count > 0 && match_count >= (keywords.len() + 1) / 2 {
                    return format!(
                        "{{\"model\":\"{}\",\"designation\":\"{}\"}}",
                        agent.model, agent.designation
                    );
                }
            }
        }

        // Default: route to the Conductor (Opus 4.6) for safety
        format!(
            "{{\"model\":\"{}\",\"designation\":\"{}\"}}",
            self.swarm.conductor.model, self.swarm.conductor.designation
        )
    }
}
