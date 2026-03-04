mod orchestrator;

use orchestrator::SamLogic;
use std::sync::Mutex;
use tauri::State;

/// Shared application state available to all Tauri commands.
struct AppState {
    sam_logic: SamLogic,
    /// Queued messages from CoPaw bridge received while the app was asleep.
    pending_resync: Mutex<Vec<String>>,
}

// ──────────────────────────────────────────────────
// Tauri Commands (IPC bridge for React frontend)
// ──────────────────────────────────────────────────

/// Returns the loaded SAM_LOGIC manifest as a JSON string.
#[tauri::command]
fn get_sam_logic(state: State<AppState>) -> String {
    serde_json::to_string_pretty(&state.sam_logic).unwrap_or_default()
}

/// Returns the current theme configuration from SAM_LOGIC.
#[tauri::command]
fn get_theme(state: State<AppState>) -> serde_json::Value {
    serde_json::json!({
        "background": state.sam_logic.ux_logic.theme.background,
        "accent": state.sam_logic.ux_logic.theme.accent,
    })
}

/// Routes a user intent to the correct swarm model.
/// Returns which model designation should handle the task.
#[tauri::command]
fn route_intent(intent: &str, state: State<AppState>) -> String {
    state.sam_logic.route(intent)
}

/// Called by the frontend when the app wakes from sleep.
/// Returns all queued messages and clears the pending queue.
#[tauri::command]
fn resync_messages(state: State<AppState>) -> Vec<String> {
    let mut queue = state.pending_resync.lock().unwrap();
    let messages = queue.drain(..).collect();
    messages
}

/// Simulates queuing a message (for dev/testing).
#[tauri::command]
fn queue_message(message: String, state: State<AppState>) {
    let mut queue = state.pending_resync.lock().unwrap();
    queue.push(message);
}

// ──────────────────────────────────────────────────
// Application Entry
// ──────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load SAM_LOGIC.yaml from the src-tauri directory at build time
    let sam_logic_yaml = include_str!("../SAM_LOGIC.yaml");
    let sam_logic: SamLogic =
        serde_yaml::from_str(sam_logic_yaml).expect("Failed to parse SAM_LOGIC.yaml");

    println!("🧠 S-ION Engine v{} initialized", sam_logic.version);
    println!("🛡️  Guardian Model: {}", sam_logic.guardian_model);

    let app_state = AppState {
        sam_logic,
        pending_resync: Mutex::new(Vec::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_sam_logic,
            get_theme,
            route_intent,
            resync_messages,
            queue_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running S-ION");
}
