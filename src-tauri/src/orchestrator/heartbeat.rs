use serde::{Deserialize, Serialize};
use std::env;
use std::sync::{Arc, Mutex};

// ──────────────────────────────────────────────────
// Bridge Mission (received from the Dispatcher queue)
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: i64,
    pub source: String, // "whatsapp", "imessage", "webhook"
    pub sender: String, // "Grandma", "Sam", etc.
    pub intent: String, // The task/question
    pub payload: Option<String>,
    pub status: String,
    pub created_at: String,
    pub claimed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DequeueResponse {
    status: String,
    mission: Option<Mission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingResponse {
    pending: i64,
}

// ──────────────────────────────────────────────────
// Bridge Heartbeat: Secure connection to Railway
// ──────────────────────────────────────────────────

pub struct BridgeHeartbeat {
    pub bridge_url: String,
    pub bridge_token: String,
    pub pending_missions: Arc<Mutex<Vec<Mission>>>,
}

impl BridgeHeartbeat {
    /// Initialize from SAM_LOGIC sentinel config + env var.
    pub fn new(bridge_url: &str) -> Self {
        let token = env::var("SION_BRIDGE_TOKEN").unwrap_or_default();

        if bridge_url.is_empty() {
            println!("🌉 Bridge Heartbeat: No bridge URL configured (offline mode)");
        } else {
            println!("🌉 Bridge Heartbeat initialized: {}", bridge_url);
        }

        Self {
            bridge_url: bridge_url.to_string(),
            bridge_token: token,
            pending_missions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The "Secret Handshake": validates that the local S-ION and the Railway
    /// bridge recognize each other before any data is exchanged.
    /// Returns true if the bridge is reachable and authenticated.
    pub async fn handshake(&self) -> Result<bool, String> {
        if self.bridge_url.is_empty() {
            return Ok(false);
        }

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{}/health", self.bridge_url))
            .send()
            .await
            .map_err(|e| format!("Bridge unreachable: {}", e))?;

        if !res.status().is_success() {
            return Err(format!("Bridge health check failed: {}", res.status()));
        }

        // Verify auth by hitting the authenticated /bridge/pending endpoint
        let auth_res = client
            .get(format!("{}/bridge/pending", self.bridge_url))
            .header("Authorization", format!("Bearer {}", self.bridge_token))
            .send()
            .await
            .map_err(|e| format!("Bridge auth check failed: {}", e))?;

        match auth_res.status().as_u16() {
            200 => {
                println!("🤝 Bridge handshake: SUCCESS (authenticated)");
                Ok(true)
            }
            401 => {
                println!("🚫 Bridge handshake: FAILED (invalid token)");
                Err("Bridge rejected our SION_BRIDGE_TOKEN: tokens don't match".into())
            }
            code => Err(format!("Bridge handshake unexpected status: {}", code)),
        }
    }

    /// Heartbeat pulse: check for and claim the next pending mission.
    /// Called periodically by the S-ION runtime (every few seconds).
    pub async fn pulse(&self) -> Result<Option<Mission>, String> {
        if self.bridge_url.is_empty() {
            return Ok(None);
        }

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{}/bridge/dequeue", self.bridge_url))
            .header("Authorization", format!("Bearer {}", self.bridge_token))
            .send()
            .await
            .map_err(|e| format!("Heartbeat failed: {}", e))?;

        if !res.status().is_success() {
            return Err(format!("Bridge returned: {}", res.status()));
        }

        let body: DequeueResponse = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse mission: {}", e))?;

        if let Some(mission) = body.mission {
            println!(
                "📨 Mission received: #{} from {} [{}]: \"{}\"",
                mission.id,
                mission.sender,
                mission.source,
                if mission.intent.len() > 50 {
                    format!("{}...", &mission.intent[..50])
                } else {
                    mission.intent.clone()
                }
            );

            // Store in local pending queue
            if let Ok(mut queue) = self.pending_missions.lock() {
                queue.push(mission.clone());
            }

            Ok(Some(mission))
        } else {
            Ok(None) // Queue empty, no missions
        }
    }

    /// Check how many missions are waiting on the bridge.
    pub async fn check_pending(&self) -> Result<i64, String> {
        if self.bridge_url.is_empty() {
            return Ok(0);
        }

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{}/bridge/pending", self.bridge_url))
            .header("Authorization", format!("Bearer {}", self.bridge_token))
            .send()
            .await
            .map_err(|e| format!("Pending check failed: {}", e))?;

        let body: PendingResponse = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse pending count: {}", e))?;

        Ok(body.pending)
    }

    /// Get locally cached missions that have been pulled from the bridge.
    pub fn get_local_missions(&self) -> Vec<Mission> {
        self.pending_missions
            .lock()
            .map(|q| q.clone())
            .unwrap_or_default()
    }
}
