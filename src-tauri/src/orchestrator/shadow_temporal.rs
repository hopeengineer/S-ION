use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

// ──────────────────────────────────────────────────
// Hot Spot Data
// ──────────────────────────────────────────────────

/// A single file's churn data from git history.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct HotSpot {
    /// Relative file path
    pub file: String,
    /// Number of commits touching this file in the last 30 days
    pub edits_30d: u32,
    /// Human-readable time since last modification
    pub last_modified: String,
    /// Risk level: "high" (>15 edits), "medium" (5-15), "low" (<5)
    pub risk: String,
}

/// The complete hot spots report for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct HotSpotsReport {
    /// Total commits in the last 30 days
    pub total_commits_30d: u32,
    /// Files sorted by edit count (descending)
    pub spots: Vec<HotSpot>,
}

// ──────────────────────────────────────────────────
// Git Temporal Analysis
// ──────────────────────────────────────────────────

/// Analyze git history to find high-churn "hot spot" files.
pub fn analyze_hot_spots(root: &Path) -> Result<HotSpotsReport, String> {
    // Check if this is a git repo
    if !root.join(".git").exists() {
        return Ok(HotSpotsReport {
            total_commits_30d: 0,
            spots: Vec::new(),
        });
    }

    println!("🔥 Analyzing git hot spots: {}", root.display());

    // Get file-level churn for the last 30 days
    let output = Command::new("git")
        .args([
            "log",
            "--name-only",
            "--format=",
            "--since=30 days ago",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to run git log: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Count occurrences of each file
    let mut file_counts: HashMap<String, u32> = HashMap::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            *file_counts.entry(trimmed.to_string()).or_insert(0) += 1;
        }
    }

    // Get total commit count for the last 30 days
    let commit_output = Command::new("git")
        .args(["rev-list", "--count", "--since=30 days ago", "HEAD"])
        .current_dir(root)
        .output()
        .ok();

    let total_commits = commit_output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // Get last modified time for each file
    let mut spots: Vec<HotSpot> = file_counts
        .into_iter()
        .map(|(file, count)| {
            let last_modified = get_last_modified(root, &file);
            let risk = if count > 15 {
                "high".to_string()
            } else if count >= 5 {
                "medium".to_string()
            } else {
                "low".to_string()
            };

            HotSpot {
                file,
                edits_30d: count,
                last_modified,
                risk,
            }
        })
        .collect();

    // Sort by edit count (descending)
    spots.sort_by(|a, b| b.edits_30d.cmp(&a.edits_30d));

    // Cap at top 50 files
    spots.truncate(50);

    println!(
        "🔥 Found {} hot spots ({} total commits in 30d), top: {} ({}x)",
        spots.len(),
        total_commits,
        spots.first().map(|s| s.file.as_str()).unwrap_or("none"),
        spots.first().map(|s| s.edits_30d).unwrap_or(0),
    );

    Ok(HotSpotsReport {
        total_commits_30d: total_commits,
        spots,
    })
}

/// Get a human-readable "last modified" time for a file using git log.
fn get_last_modified(root: &Path, file: &str) -> String {
    let output = Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%ar",
            "--",
            file,
        ])
        .current_dir(root)
        .output()
        .ok();

    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
