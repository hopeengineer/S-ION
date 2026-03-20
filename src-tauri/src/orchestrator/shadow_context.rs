use std::fs;
use std::path::Path;

// ──────────────────────────────────────────────────
// Selective Context Loader (Pyramid of Context)
// ──────────────────────────────────────────────────
//
// Base Layer (always loaded):   ARCHITECTURE.md + STACK.md       (~600 tokens)
// Targeted Layer (keyword):     STATE.md / PATTERNS.md / GOTCHAS.md  (~200-400 each)
// Atlas Layer (on file edit):   relevant ATLAS.json slice          (~200 tokens)
// Hot Spot Layer (on edit):     relevant HOTSPOTS.json entry       (~50 tokens)

/// Keywords that trigger loading specific shadow docs.
const STATE_TRIGGERS: &[&str] = &[
    "state", "store", "redux", "context", "database", "db", "schema",
    "mutation", "reducer", "zustand", "atom", "signal", "reactive",
    "useState", "setState", "AppState", "table", "column", "migration",
];

const PATTERNS_TRIGGERS: &[&str] = &[
    "pattern", "convention", "style", "naming", "architecture",
    "refactor", "structure", "organize", "best practice", "anti-pattern",
    "folder", "layout", "design pattern",
];

const GOTCHAS_TRIGGERS: &[&str] = &[
    "bug", "error", "issue", "fix", "problem", "broken", "crash",
    "gotcha", "pitfall", "warning", "fail", "wrong", "unexpected",
    "workaround", "hack", "quirk",
];

/// Build context to prepend to an agent's system prompt.
/// Implements the Pyramid of Context: base always, targeted on keyword match.
pub fn build_context_for_prompt(workspace_path: &str, user_intent: &str) -> String {
    let shadow_dir = Path::new(workspace_path).join(".sion-shadow");

    if !shadow_dir.exists() {
        return String::new(); // No shadow docs yet
    }

    let mut context = String::new();
    let intent_lower = user_intent.to_lowercase();

    // ── Base Layer (always loaded) ──
    context.push_str("──── Project Context (Auto-Shadow) ────\n\n");

    if let Some(arch) = read_shadow_doc(&shadow_dir, "ARCHITECTURE.md") {
        context.push_str(&arch);
        context.push_str("\n\n");
    }
    if let Some(stack) = read_shadow_doc(&shadow_dir, "STACK.md") {
        context.push_str(&stack);
        context.push_str("\n\n");
    }

    // ── Targeted Layer (keyword-triggered) ──
    if matches_any(&intent_lower, STATE_TRIGGERS) {
        if let Some(state) = read_shadow_doc(&shadow_dir, "STATE.md") {
            context.push_str(&state);
            context.push_str("\n\n");
        }
    }

    if matches_any(&intent_lower, PATTERNS_TRIGGERS) {
        if let Some(patterns) = read_shadow_doc(&shadow_dir, "PATTERNS.md") {
            context.push_str(&patterns);
            context.push_str("\n\n");
        }
    }

    if matches_any(&intent_lower, GOTCHAS_TRIGGERS) {
        if let Some(gotchas) = read_shadow_doc(&shadow_dir, "GOTCHAS.md") {
            context.push_str(&gotchas);
            context.push_str("\n\n");
        }
    }

    // ── Hot Spot Layer ──
    // If the intent mentions specific files, check if they're hot spots
    if let Some(hotspots_json) = read_shadow_doc(&shadow_dir, "HOTSPOTS.json") {
        if let Ok(report) = serde_json::from_str::<crate::orchestrator::shadow_temporal::HotSpotsReport>(&hotspots_json) {
            let hot_files: Vec<&crate::orchestrator::shadow_temporal::HotSpot> = report.spots.iter()
                .filter(|s| s.risk == "high" && intent_lower.contains(&s.file.to_lowercase()))
                .collect();

            if !hot_files.is_empty() {
                context.push_str("⚠️ HOT SPOT WARNING: The following files have high churn:\n");
                for spot in hot_files {
                    context.push_str(&format!(
                        "  🔥 {} — {} edits in 30d (last: {}). Be cautious.\n",
                        spot.file, spot.edits_30d, spot.last_modified
                    ));
                }
                context.push('\n');
            }
        }
    }

    if context.len() > "──── Project Context (Auto-Shadow) ────\n\n".len() {
        context.push_str("──── End Project Context ────\n");
        println!(
            "🧠 Context Loader: {} chars loaded ({} base + targeted)",
            context.len(),
            if matches_any(&intent_lower, STATE_TRIGGERS) { "+STATE" }
            else if matches_any(&intent_lower, PATTERNS_TRIGGERS) { "+PATTERNS" }
            else if matches_any(&intent_lower, GOTCHAS_TRIGGERS) { "+GOTCHAS" }
            else { "base only" }
        );
        context
    } else {
        String::new() // No docs loaded
    }
}

/// Read a shadow doc file, returning None if it doesn't exist or is empty.
fn read_shadow_doc(shadow_dir: &Path, filename: &str) -> Option<String> {
    let path = shadow_dir.join(filename);
    fs::read_to_string(&path)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Check if the intent contains any of the trigger words.
fn matches_any(intent: &str, triggers: &[&str]) -> bool {
    triggers.iter().any(|t| intent.contains(&t.to_lowercase()))
}
