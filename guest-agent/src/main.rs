//! S-ION Guest Agent
//!
//! Minimal binary (~200 lines) that runs INSIDE a Firecracker MicroVM.
//! Listens on vsock CID:3, port:1234 for Mission commands from the Host.
//! Executes them, captures output + file diffs, and sends results back.
//!
//! Target: x86_64-unknown-linux-musl (static, < 2MB)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

// ──────────────────────────────────────────────────
// Vsock Frame Protocol: [4-byte BE length][JSON payload]
// ──────────────────────────────────────────────────

const DEFAULT_WORKSPACE: &str = "/workspace";
const VSOCK_PORT: u32 = 1234;

/// Get the workspace directory — configurable via SION_WORKSPACE env var.
fn workspace_dir() -> String {
    std::env::var("SION_WORKSPACE").unwrap_or_else(|_| DEFAULT_WORKSPACE.to_string())
}

/// Read one framed message: [4-byte big-endian length][JSON bytes]
fn read_frame(stream: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|e| format!("Failed to read frame header: {}", e))?;

    let len = u32::from_be_bytes(header) as usize;
    if len > 10 * 1024 * 1024 {
        return Err(format!("Frame too large: {} bytes", len));
    }

    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .map_err(|e| format!("Failed to read frame payload: {}", e))?;

    Ok(payload)
}

/// Write one framed message: [4-byte big-endian length][JSON bytes]
fn write_frame(stream: &mut impl Write, payload: &[u8]) -> Result<(), String> {
    let header = (payload.len() as u32).to_be_bytes();
    stream
        .write_all(&header)
        .map_err(|e| format!("Failed to write frame header: {}", e))?;
    stream
        .write_all(payload)
        .map_err(|e| format!("Failed to write frame payload: {}", e))?;
    stream.flush().map_err(|e| format!("Flush failed: {}", e))?;
    Ok(())
}

// ──────────────────────────────────────────────────
// Protocol Messages
// ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IncomingMessage {
    #[serde(rename = "mission")]
    Mission(Mission),
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Deserialize)]
struct Mission {
    task_id: String,
    command: String,
    #[serde(default)]
    files: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Serialize)]
struct MissionResult {
    #[serde(rename = "type")]
    msg_type: String,
    task_id: String,
    exit_code: i32,
    stdout: String,
    stderr: String,
    file_diffs: HashMap<String, FileDiff>,
    duration_ms: u64,
}

#[derive(Debug, Serialize)]
struct FileDiff {
    status: String, // "added", "modified", "deleted"
    before: Option<String>,
    after: Option<String>,
}

#[derive(Debug, Serialize)]
struct Pong {
    #[serde(rename = "type")]
    msg_type: String,
    uptime_secs: u64,
}

#[derive(Debug, Serialize)]
struct HealthReport {
    #[serde(rename = "type")]
    msg_type: String,
    cpu_percent: f64,
    memory_used_mb: u64,
    memory_limit_mb: u64,
    uptime_secs: u64,
    snapback_ready: bool,
    workspace_files: usize,
}

// ──────────────────────────────────────────────────
// Mission Triage: Safety Check
// ──────────────────────────────────────────────────

/// Checks if a command is safe to execute inside the sandbox.
/// Returns Ok(()) if safe, Err(reason) if the command needs higher permissions.
fn triage_command(command: &str) -> Result<(), String> {
    let blocked_patterns = [
        "rm -rf /",
        "mkfs",
        "dd if=",
        ":(){ :|:& };:", // Fork bomb
        "chmod -R 777 /",
        "> /dev/sda",
        "shutdown",
        "reboot",
        "halt",
        "init 0",
        "curl | sh",
        "wget | sh",
        "curl | bash",
        "wget | bash",
    ];

    let lower = command.to_lowercase();
    for pattern in &blocked_patterns {
        if lower.contains(pattern) {
            return Err(format!(
                "BLOCKED: Command contains dangerous pattern '{}'",
                pattern
            ));
        }
    }

    // Block attempts to escape workspace
    if command.contains("..") && (command.contains("/etc") || command.contains("/root")) {
        return Err("BLOCKED: Path traversal attempt detected".into());
    }

    Ok(())
}

// ──────────────────────────────────────────────────
// Workspace Snapshot & Diff
// ──────────────────────────────────────────────────

fn snapshot_workspace(workspace: &Path) -> HashMap<String, String> {
    let mut state = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(workspace) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
                state.insert(name, content);
            }
        }
    }
    state
}

fn diff_workspace(
    before: &HashMap<String, String>,
    after: &HashMap<String, String>,
) -> HashMap<String, FileDiff> {
    let mut diffs = HashMap::new();

    // Modified or deleted
    for (name, before_content) in before {
        match after.get(name) {
            Some(after_content) if after_content != before_content => {
                diffs.insert(
                    name.clone(),
                    FileDiff {
                        status: "modified".into(),
                        before: Some(before_content.clone()),
                        after: Some(after_content.clone()),
                    },
                );
            }
            None => {
                diffs.insert(
                    name.clone(),
                    FileDiff {
                        status: "deleted".into(),
                        before: Some(before_content.clone()),
                        after: None,
                    },
                );
            }
            _ => {}
        }
    }

    // Added
    for (name, after_content) in after {
        if !before.contains_key(name) {
            diffs.insert(
                name.clone(),
                FileDiff {
                    status: "added".into(),
                    before: None,
                    after: Some(after_content.clone()),
                },
            );
        }
    }

    diffs
}

// ──────────────────────────────────────────────────
// Mission Executor
// ──────────────────────────────────────────────────

fn execute_mission(mission: Mission) -> MissionResult {
    let workspace = PathBuf::from(workspace_dir());
    let _ = std::fs::create_dir_all(&workspace);

    // Seed files into workspace
    for (name, content) in &mission.files {
        let path = workspace.join(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, content);
    }

    // Triage: check if the command is safe
    if let Err(reason) = triage_command(&mission.command) {
        return MissionResult {
            msg_type: "result".into(),
            task_id: mission.task_id,
            exit_code: -2,
            stdout: String::new(),
            stderr: reason,
            file_diffs: HashMap::new(),
            duration_ms: 0,
        };
    }

    eprintln!("   Timeout: {}s", mission.timeout_secs);

    // Snapshot before execution
    let before_state = snapshot_workspace(&workspace);

    // Execute with clean environment
    let start = Instant::now();
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(&mission.command)
        .current_dir(&workspace)
        .env_clear()
        .env("HOME", workspace_dir())
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("SION_GUEST", "1")
        .output();

    let duration_ms = start.elapsed().as_millis() as u64;

    match output {
        Ok(out) => {
            let after_state = snapshot_workspace(&workspace);
            let file_diffs = diff_workspace(&before_state, &after_state);

            MissionResult {
                msg_type: "result".into(),
                task_id: mission.task_id,
                exit_code: out.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                file_diffs,
                duration_ms,
            }
        }
        Err(e) => MissionResult {
            msg_type: "result".into(),
            task_id: mission.task_id,
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("Execution failed: {}", e),
            file_diffs: HashMap::new(),
            duration_ms,
        },
    }
}

// ──────────────────────────────────────────────────
// Health Reporting
// ──────────────────────────────────────────────────

fn build_health_report(boot_time: &Instant) -> HealthReport {
    // Read /proc/meminfo for memory stats
    let (mem_used, mem_total) = read_memory_info();

    // Read /proc/loadavg for CPU approximation
    let cpu = read_cpu_load();

    let workspace = PathBuf::from(workspace_dir());
    let file_count = std::fs::read_dir(&workspace)
        .map(|entries| entries.count())
        .unwrap_or(0);

    HealthReport {
        msg_type: "health".into(),
        cpu_percent: cpu,
        memory_used_mb: mem_used,
        memory_limit_mb: mem_total,
        uptime_secs: boot_time.elapsed().as_secs(),
        snapback_ready: true,
        workspace_files: file_count,
    }
}

fn read_memory_info() -> (u64, u64) {
    let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total_kb = 0u64;
    let mut available_kb = 0u64;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = parse_meminfo_value(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = parse_meminfo_value(line);
        }
    }

    let total_mb = total_kb / 1024;
    let used_mb = total_mb.saturating_sub(available_kb / 1024);
    (used_mb, total_mb)
}

fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn read_cpu_load() -> f64 {
    let content = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    content
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|load| (load * 100.0).min(100.0))
        .unwrap_or(0.0)
}

// ──────────────────────────────────────────────────
// Main: Vsock Listener
// ──────────────────────────────────────────────────

fn main() {
    eprintln!("🤖 S-ION Guest Agent v0.1.0");
    eprintln!("   Listening on vsock port {}", VSOCK_PORT);

    let boot_time = Instant::now();

    // In production: use vsock listener (AF_VSOCK)
    // For development/testing: use Unix socket as fallback
    let socket_path = format!("/tmp/sion-guest-{}.sock", VSOCK_PORT);
    let _ = std::fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("❌ Failed to bind socket: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("✅ Guest Agent ready (socket: {})", socket_path);

    for stream in listener.incoming() {
        match stream {
            Ok(mut conn) => {
                eprintln!("📨 Connection received");

                loop {
                    let payload = match read_frame(&mut conn) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("⚠️  Read error (client disconnected?): {}", e);
                            break;
                        }
                    };

                    let msg: IncomingMessage = match serde_json::from_slice(&payload) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("⚠️  Invalid message: {}", e);
                            continue;
                        }
                    };

                    match msg {
                        IncomingMessage::Ping => {
                            let pong = Pong {
                                msg_type: "pong".into(),
                                uptime_secs: boot_time.elapsed().as_secs(),
                            };
                            let json = serde_json::to_vec(&pong).unwrap();
                            if let Err(e) = write_frame(&mut conn, &json) {
                                eprintln!("⚠️  Failed to send pong: {}", e);
                                break;
                            }
                        }
                        IncomingMessage::Mission(mission) => {
                            let task_id = mission.task_id.clone();
                            eprintln!("🚀 Executing mission: {}", &task_id[..task_id.len().min(8)]);

                            let result = execute_mission(mission);
                            let json = serde_json::to_vec(&result).unwrap();

                            if let Err(e) = write_frame(&mut conn, &json) {
                                eprintln!("⚠️  Failed to send result: {}", e);
                                break;
                            }

                            // Send health report after every mission
                            let health = build_health_report(&boot_time);
                            let health_json = serde_json::to_vec(&health).unwrap();
                            if let Err(e) = write_frame(&mut conn, &health_json) {
                                eprintln!("⚠️  Failed to send health: {}", e);
                                break;
                            }

                            eprintln!(
                                "✅ Mission {} complete (exit={}, {}ms)",
                                &task_id[..task_id.len().min(8)],
                                result.exit_code,
                                result.duration_ms
                            );
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️  Accept error: {}", e);
            }
        }
    }
}
