use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ──────────────────────────────────────────────────
// Frame Protocol: [4-byte big-endian length][JSON payload]
// ──────────────────────────────────────────────────

const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Read one framed message from an async reader.
pub async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|e| format!("Failed to read frame header: {}", e))?;

    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(format!(
            "Frame too large: {} bytes (max {})",
            len, MAX_FRAME_SIZE
        ));
    }

    let mut payload = vec![0u8; len];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|e| format!("Failed to read frame payload ({} bytes): {}", len, e))?;

    Ok(payload)
}

/// Write one framed message to an async writer.
pub async fn write_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> Result<(), String> {
    let header = (payload.len() as u32).to_be_bytes();
    writer
        .write_all(&header)
        .await
        .map_err(|e| format!("Failed to write frame header: {}", e))?;
    writer
        .write_all(payload)
        .await
        .map_err(|e| format!("Failed to write frame payload: {}", e))?;
    writer
        .flush()
        .await
        .map_err(|e| format!("Flush failed: {}", e))?;
    Ok(())
}

// ──────────────────────────────────────────────────
// Protocol Messages: Host → Guest
// ──────────────────────────────────────────────────

/// A mission sent from S-ION Host to the Guest Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockMission {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub task_id: String,
    pub command: String,
    #[serde(default)]
    pub files: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub network_enabled: bool,
}

fn default_timeout() -> u64 {
    30
}

impl VsockMission {
    pub fn new(task_id: String, command: String) -> Self {
        Self {
            msg_type: "mission".into(),
            task_id,
            command,
            files: HashMap::new(),
            timeout_secs: 30,
            network_enabled: false,
        }
    }

    pub fn with_files(mut self, files: HashMap<String, String>) -> Self {
        self.files = files;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Serialize to framed bytes for transmission.
    pub fn to_frame(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("Failed to serialize mission: {}", e))
    }
}

/// Ping message for heartbeat checks.
#[derive(Debug, Serialize)]
pub struct VsockPing {
    #[serde(rename = "type")]
    pub msg_type: String,
}

impl VsockPing {
    pub fn new() -> Self {
        Self {
            msg_type: "ping".into(),
        }
    }

    pub fn to_frame(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("Failed to serialize ping: {}", e))
    }
}

// ──────────────────────────────────────────────────
// Protocol Messages: Guest → Host
// ──────────────────────────────────────────────────

/// Result from a mission execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockResult {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub task_id: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub file_diffs: HashMap<String, VsockFileDiff>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockFileDiff {
    pub status: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Pong response to a ping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockPong {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub uptime_secs: u64,
}

/// Health report from the Guest Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarHealth {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub memory_limit_mb: u64,
    pub uptime_secs: u64,
    pub snapback_ready: bool,
    pub workspace_files: usize,
}

// ──────────────────────────────────────────────────
// Incoming Message Discriminator
// ──────────────────────────────────────────────────

/// All possible messages the host can receive from the guest.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum GuestMessage {
    #[serde(rename = "result")]
    Result(VsockResult),
    #[serde(rename = "pong")]
    Pong(VsockPong),
    #[serde(rename = "health")]
    Health(SidecarHealth),
}

/// Parse a raw payload into a typed guest message.
pub fn parse_guest_message(payload: &[u8]) -> Result<GuestMessage, String> {
    serde_json::from_slice(payload).map_err(|e| format!("Failed to parse guest message: {}", e))
}

// ──────────────────────────────────────────────────
// Vsock Channel: Host-side connection manager
// ──────────────────────────────────────────────────

/// Manages a framed connection to the Guest Agent.
#[derive(Clone)]
pub struct VsockChannel {
    /// Guest Context ID (default: 3 for Firecracker)
    pub cid: u32,
    /// Service port (default: 1234)
    pub port: u32,
    /// Whether the channel is currently connected.
    pub connected: bool,
    /// Last health report from guest.
    pub last_health: Option<SidecarHealth>,
}

impl VsockChannel {
    pub fn new() -> Self {
        Self {
            cid: 3,
            port: 1234,
            connected: false,
            last_health: None,
        }
    }

    /// Send a mission over the channel and wait for the result.
    /// On macOS, this falls back to Unix socket for development.
    pub async fn send_mission(&mut self, mission: VsockMission) -> Result<VsockResult, String> {
        println!(
            "📡 Vsock: sending mission to CID:{} port:{}",
            self.cid, self.port
        );
        let socket_path = format!("/tmp/sion-guest-{}.sock", self.port);

        let mut stream = tokio::net::UnixStream::connect(&socket_path)
            .await
            .map_err(|e| format!("Failed to connect to guest agent: {}", e))?;

        self.connected = true;

        // Send the mission
        let payload = mission.to_frame()?;
        write_frame(&mut stream, &payload).await?;

        // Read the result
        let result_payload = read_frame(&mut stream).await?;
        let guest_msg = parse_guest_message(&result_payload)?;

        let result = match guest_msg {
            GuestMessage::Result(r) => r,
            _ => return Err("Expected result message, got something else".into()),
        };

        // Try to read the health report that follows
        if let Ok(health_payload) = read_frame(&mut stream).await {
            if let Ok(GuestMessage::Health(h)) = parse_guest_message(&health_payload) {
                self.last_health = Some(h);
            }
        }

        Ok(result)
    }

    /// Ping the guest agent to check if it's alive.
    pub async fn ping(&mut self) -> Result<VsockPong, String> {
        let socket_path = format!("/tmp/sion-guest-{}.sock", self.port);

        let mut stream = tokio::net::UnixStream::connect(&socket_path)
            .await
            .map_err(|e| format!("Failed to connect for ping: {}", e))?;

        self.connected = true;

        let ping = VsockPing::new();
        let payload = ping.to_frame()?;
        write_frame(&mut stream, &payload).await?;

        let response = read_frame(&mut stream).await?;
        let msg = parse_guest_message(&response)?;

        match msg {
            GuestMessage::Pong(p) => Ok(p),
            _ => Err("Expected pong, got something else".into()),
        }
    }

    /// Get the most recent health snapshot.
    pub fn get_health(&self) -> Option<&SidecarHealth> {
        self.last_health.as_ref()
    }
}
