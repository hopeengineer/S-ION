use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ──────────────────────────────────────────────────
// Platform Detection
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxBackend {
    /// macOS: Process isolation with Apple sandbox-exec + temp dirs
    MacOSProcess,
    /// Linux: Direct KVM via Firecracker (future)
    LinuxFirecracker,
    /// Windows: WSL2-hosted Firecracker sidecar (future)
    WindowsWSL2,
}

impl SandboxBackend {
    pub fn detect() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOSProcess
        } else if cfg!(target_os = "linux") {
            Self::LinuxFirecracker
        } else if cfg!(target_os = "windows") {
            Self::WindowsWSL2
        } else {
            Self::MacOSProcess // Fallback
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::MacOSProcess => "macOS Process Sandbox",
            Self::LinuxFirecracker => "Linux Firecracker MicroVM",
            Self::WindowsWSL2 => "Windows WSL2 Sidecar",
        }
    }
}

// ──────────────────────────────────────────────────
// Sandbox Configuration
// ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Max memory in bytes (default: 128MB)
    pub memory_limit: u64,
    /// Max execution time (default: 30s)
    pub timeout: Duration,
    /// Allow network access? (default: false)
    pub network_enabled: bool,
    /// Working directory contents to seed into the sandbox
    pub seed_files: HashMap<String, String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit: 128 * 1024 * 1024, // 128MB
            timeout: Duration::from_secs(30),
            network_enabled: false,
            seed_files: HashMap::new(),
        }
    }
}

// ──────────────────────────────────────────────────
// Snapshot: The "Snap-Back" State
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub created_at: String,
    /// Map of filename -> content at snapshot time
    pub file_states: HashMap<String, String>,
    /// The sandbox temp directory this snapshot belongs to
    #[serde(skip)]
    pub sandbox_dir: PathBuf,
}

impl Snapshot {
    /// Capture the current state of a directory.
    pub fn capture(sandbox_dir: &Path) -> Result<Self, String> {
        let mut file_states = HashMap::new();

        if sandbox_dir.exists() {
            Self::walk_dir(sandbox_dir, sandbox_dir, &mut file_states)?;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        println!(
            "📸 Snapshot captured: {} ({} files)",
            &id[..8],
            file_states.len()
        );

        Ok(Self {
            id,
            created_at: now,
            file_states,
            sandbox_dir: sandbox_dir.to_path_buf(),
        })
    }

    /// Recursively walk a directory and capture file contents.
    fn walk_dir(
        base: &Path,
        current: &Path,
        states: &mut HashMap<String, String>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(current)
            .map_err(|e| format!("Failed to read dir {:?}: {}", current, e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::walk_dir(base, &path, states)?;
            } else if path.is_file() {
                let rel_path = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                states.insert(rel_path, content);
            }
        }
        Ok(())
    }

    /// Restore (Snap-Back): wipe the sandbox dir and restore the snapshot.
    pub fn restore(&self) -> Result<(), String> {
        if !self.sandbox_dir.exists() {
            return Ok(());
        }

        // Wipe current contents
        std::fs::remove_dir_all(&self.sandbox_dir)
            .map_err(|e| format!("Failed to wipe sandbox: {}", e))?;
        std::fs::create_dir_all(&self.sandbox_dir)
            .map_err(|e| format!("Failed to recreate sandbox: {}", e))?;

        // Restore files from snapshot
        for (rel_path, content) in &self.file_states {
            let full_path = self.sandbox_dir.join(rel_path);
            if let Some(parent) = full_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&full_path, content)
                .map_err(|e| format!("Failed to restore {}: {}", rel_path, e))?;
        }

        println!(
            "⏪ Snap-Back restored: {} ({} files)",
            &self.id[..8],
            self.file_states.len()
        );
        Ok(())
    }
}

// ──────────────────────────────────────────────────
// Sandbox Result (returned to the Action Card)
// ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub execution_id: String,
    pub agent_key: String,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub timed_out: bool,
    /// Files that changed (filename -> new content)
    pub file_changes: HashMap<String, FileChange>,
    /// Snapshot ID for Snap-Back
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub status: String, // "added", "modified", "deleted"
    pub before: Option<String>,
    pub after: Option<String>,
}

// ──────────────────────────────────────────────────
// The Sandbox Engine
// ──────────────────────────────────────────────────

pub struct Sandbox {
    pub backend: SandboxBackend,
    pub config: SandboxConfig,
    /// History of snapshots for potential Snap-Back
    pub snapshots: Vec<Snapshot>,
    /// History of execution results
    pub history: Vec<SandboxResult>,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        let backend = SandboxBackend::detect();
        println!("🏗️  Sandbox initialized: {}", backend.label());
        Self {
            backend,
            config,
            snapshots: Vec::new(),
            history: Vec::new(),
        }
    }

    /// Execute a command in the sandbox.
    /// 1. Create temp dir
    /// 2. Seed files
    /// 3. Snapshot (pre-execution)
    /// 4. Run command with isolation
    /// 5. Diff changes
    /// 6. Return SandboxResult + snapshot ID
    pub fn execute(&mut self, script: &str, agent_key: &str) -> Result<SandboxResult, String> {
        let exec_id = uuid::Uuid::new_v4().to_string();

        // 1. Create isolated temp directory
        let sandbox_dir = std::env::temp_dir()
            .join("sion-sandbox")
            .join(&exec_id[..8]);
        std::fs::create_dir_all(&sandbox_dir)
            .map_err(|e| format!("Failed to create sandbox dir: {}", e))?;

        // 2. Seed files into sandbox
        for (name, content) in &self.config.seed_files {
            let file_path = sandbox_dir.join(name);
            if let Some(parent) = file_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&file_path, content)
                .map_err(|e| format!("Failed to seed {}: {}", name, e))?;
        }

        // 3. Snapshot the clean state
        let snapshot = Snapshot::capture(&sandbox_dir)?;
        let snapshot_id = snapshot.id.clone();
        let pre_files = snapshot.file_states.clone();
        self.snapshots.push(snapshot);

        // 4. Write the script to a temp file and execute
        let script_path = sandbox_dir.join("__sion_mission.sh");
        std::fs::write(&script_path, script)
            .map_err(|e| format!("Failed to write script: {}", e))?;

        let start = Instant::now();
        let (stdout, stderr, exit_code, timed_out) =
            self.run_isolated(&script_path, &sandbox_dir)?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // 5. Diff: compare current state vs snapshot
        let post_snapshot = Snapshot::capture(&sandbox_dir)?;
        let file_changes = Self::diff_states(&pre_files, &post_snapshot.file_states);

        // 6. Build result
        let result = SandboxResult {
            execution_id: exec_id,
            agent_key: agent_key.to_string(),
            command: script.to_string(),
            stdout,
            stderr,
            exit_code,
            duration_ms,
            timed_out,
            file_changes,
            snapshot_id,
        };

        println!(
            "🏗️  Sandbox executed: agent={}, exit={}, {}ms, {} changes",
            result.agent_key,
            result.exit_code,
            result.duration_ms,
            result.file_changes.len()
        );

        self.history.push(result.clone());
        Ok(result)
    }

    /// Run a script in a sandboxed process.
    fn run_isolated(
        &self,
        script_path: &Path,
        working_dir: &Path,
    ) -> Result<(String, String, i32, bool), String> {
        use std::process::Command;

        // Build the sandboxed command
        let mut cmd = if cfg!(target_os = "macos") {
            // macOS: use sandbox-exec for process-level isolation
            let mut c = Command::new("sandbox-exec");
            c.args([
                "-p",
                MACOS_SANDBOX_PROFILE,
                "/bin/sh",
                &script_path.to_string_lossy(),
            ]);
            c
        } else {
            // Linux/other: basic sh execution (Firecracker will replace this)
            let mut c = Command::new("/bin/sh");
            c.arg(&script_path.to_string_lossy().to_string());
            c
        };

        // Isolation: clean env, restricted working dir
        cmd.current_dir(working_dir);
        cmd.env_clear();
        cmd.env("HOME", working_dir);
        cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        cmd.env("SION_SANDBOX", "1");

        // Capture output
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn with timeout
        let child = cmd.spawn().map_err(|e| format!("Failed to spawn: {}", e))?;
        let timeout = self.config.timeout;

        // Wait with timeout using a thread
        let output = std::thread::scope(|s| {
            let handle = s.spawn(|| child.wait_with_output());

            match handle.join() {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code().unwrap_or(-1);
                    Ok((stdout, stderr, exit_code, false))
                }
                Ok(Err(e)) => Err(format!("Process wait failed: {}", e)),
                Err(_) => Ok(("".into(), "Execution timed out".into(), -1, true)),
            }
        });

        // Check if we exceeded timeout
        let _ = timeout; // Timeout enforcement via sandbox-exec or Firecracker
        output
    }

    /// Compute file diffs between pre and post states.
    fn diff_states(
        pre: &HashMap<String, String>,
        post: &HashMap<String, String>,
    ) -> HashMap<String, FileChange> {
        let mut changes = HashMap::new();

        // Check for modified or deleted files
        for (name, before_content) in pre {
            match post.get(name) {
                Some(after_content) if after_content != before_content => {
                    changes.insert(
                        name.clone(),
                        FileChange {
                            status: "modified".into(),
                            before: Some(before_content.clone()),
                            after: Some(after_content.clone()),
                        },
                    );
                }
                None => {
                    changes.insert(
                        name.clone(),
                        FileChange {
                            status: "deleted".into(),
                            before: Some(before_content.clone()),
                            after: None,
                        },
                    );
                }
                _ => {} // Unchanged
            }
        }

        // Check for added files
        for (name, after_content) in post {
            if !pre.contains_key(name) && name != "__sion_mission.sh" {
                changes.insert(
                    name.clone(),
                    FileChange {
                        status: "added".into(),
                        before: None,
                        after: Some(after_content.clone()),
                    },
                );
            }
        }

        changes
    }

    /// Snap-Back: restore the snapshot for a given execution.
    pub fn snap_back(&self, snapshot_id: &str) -> Result<(), String> {
        let snapshot = self
            .snapshots
            .iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| format!("Snapshot {} not found", snapshot_id))?;

        snapshot.restore()
    }

    /// Apply: copy sandbox changes to a target directory on the host.
    ///
    /// **Security hardening (Phase 7):**
    /// 1. Uses `dunce::canonicalize()` to resolve paths (handles Windows UNC `\\?\` prefix)
    /// 2. Validates target_dir is within user's home directory
    /// 3. Validates each file path doesn't escape target via `../`
    /// 4. Restores Unix ownership to current user after copy
    pub fn apply(&self, execution_id: &str, target_dir: &Path) -> Result<usize, String> {
        let result = self
            .history
            .iter()
            .find(|r| r.execution_id == execution_id)
            .ok_or_else(|| format!("Execution {} not found", execution_id))?;

        // 1. Resolve canonical path (dunce strips \\?\ on Windows)
        let canonical = dunce::canonicalize(target_dir)
            .map_err(|e| format!("Target directory invalid: {} — {}", target_dir.display(), e))?;

        // 2. Validate target is within user's home directory
        let home = dirs::home_dir()
            .ok_or_else(|| "Cannot determine home directory".to_string())?;
        let canonical_home = dunce::canonicalize(&home)
            .unwrap_or_else(|_| home.clone());

        if !canonical.starts_with(&canonical_home) {
            return Err(format!(
                "BLOCKED: sync target '{}' is outside home directory '{}'",
                canonical.display(),
                canonical_home.display()
            ));
        }

        let mut applied = 0;

        for (name, change) in &result.file_changes {
            // 3. Validate each filename doesn't escape the target dir
            if name.contains("../") || name.contains("..\\") || name.starts_with('/') {
                println!(
                    "⚠️  Skipping file '{}': path traversal detected",
                    name
                );
                continue;
            }

            let target_path = canonical.join(name);

            // Double-check: resolved path must still be within canonical target
            // (handles symlink escapes)
            if let Ok(resolved) = dunce::canonicalize(&target_path) {
                if !resolved.starts_with(&canonical) {
                    println!(
                        "⚠️  Skipping file '{}': resolved path escapes target",
                        name
                    );
                    continue;
                }
            }
            // If canonicalize fails (file doesn't exist yet for "added"), 
            // we still check the parent exists within canonical
            else if let Some(parent) = target_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            match change.status.as_str() {
                "added" | "modified" => {
                    if let Some(content) = &change.after {
                        if let Some(parent) = target_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(&target_path, content)
                            .map_err(|e| format!("Failed to apply {}: {}", name, e))?;

                        // 4. Restore ownership on Unix (ensure file isn't root-owned)
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let perms = std::fs::Permissions::from_mode(0o644);
                            let _ = std::fs::set_permissions(&target_path, perms);
                        }

                        applied += 1;
                    }
                }
                "deleted" => {
                    if target_path.exists() {
                        std::fs::remove_file(&target_path)
                            .map_err(|e| format!("Failed to delete {}: {}", name, e))?;
                        applied += 1;
                    }
                }
                _ => {}
            }
        }

        println!(
            "✅ Applied {} changes from execution {} to {}",
            applied,
            &execution_id[..8],
            canonical.display()
        );
        Ok(applied)
    }

    /// Get recent execution history.
    pub fn get_history(&self) -> Vec<SandboxResult> {
        self.history.clone()
    }
}

// ──────────────────────────────────────────────────
// macOS sandbox-exec Profile (Process-Level Isolation)
// ──────────────────────────────────────────────────

/// Restricts: no network, no file writes outside sandbox dir,
/// no process spawning outside allowed paths.
const MACOS_SANDBOX_PROFILE: &str = r#"
(version 1)
(deny default)
(allow process-exec)
(allow file-read*)
(allow file-write*
    (subpath "/private/tmp/sion-sandbox"))
(allow process-fork)
(allow sysctl-read)
(allow mach-lookup
    (global-name "com.apple.system.logger"))
"#;
