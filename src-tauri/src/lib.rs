mod orchestrator;

use orchestrator::egress::EgressFilter;
use orchestrator::heartbeat::BridgeHeartbeat;
use orchestrator::router::{self, DispatchResult, ExpertPins, RuntimeMode};
use orchestrator::sandbox::{Sandbox, SandboxConfig};
use orchestrator::sentinel::Sentinel;
use orchestrator::{PipelineResult, SamLogic};
use std::sync::Mutex;
use tauri::State;

/// Shared application state available to all Tauri commands.
struct AppState {
    sam_logic: SamLogic,
    /// Current runtime mode: Smart (auto-dispatch) or Expert (manual pins).
    mode: Mutex<RuntimeMode>,
    /// Expert Mode: user-defined model pins per task category.
    expert_pins: Mutex<ExpertPins>,
    /// Queued messages from CoPaw bridge received while the app was asleep.
    pending_resync: Mutex<Vec<String>>,
    /// Egress Filter: domain allowlist gate for all outgoing AI agent requests.
    egress: Mutex<EgressFilter>,
    /// Sentinel: privacy-preserving telemetry with Triple-Pass PII scrubber.
    sentinel: Sentinel,
    /// Sandbox: Firecracker-ready code isolation with Snap-Back.
    sandbox: Mutex<Sandbox>,
    /// Bridge Heartbeat: secure connection to Railway for CoPaw missions.
    heartbeat: BridgeHeartbeat,
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

/// Gets the current runtime mode (Smart or Expert).
#[tauri::command]
fn get_mode(state: State<AppState>) -> String {
    let mode = state.mode.lock().unwrap();
    serde_json::to_string(&*mode).unwrap_or_default()
}

/// Sets the runtime mode (Smart or Expert).
#[tauri::command]
fn set_mode(mode_str: String, state: State<AppState>) -> String {
    let new_mode = match mode_str.as_str() {
        "smart" | "Smart" => RuntimeMode::Smart,
        "expert" | "Expert" => RuntimeMode::Expert,
        _ => return "Invalid mode. Use 'smart' or 'expert'.".into(),
    };
    let mut current = state.mode.lock().unwrap();
    *current = new_mode.clone();
    let label = match new_mode {
        RuntimeMode::Smart => "Smart",
        RuntimeMode::Expert => "Expert",
    };
    println!("🔄 Mode switched to: {}", label);
    format!("Mode set to: {}", label)
}

/// Gets the current Expert Mode model pins.
#[tauri::command]
fn get_model_pins(state: State<AppState>) -> String {
    let pins = state.expert_pins.lock().unwrap();
    serde_json::to_string(&pins.pins).unwrap_or_default()
}

/// Sets a model pin for a specific task category in Expert Mode.
#[tauri::command]
fn set_model_pin(category: String, agent_key: String, state: State<AppState>) -> String {
    let mut pins = state.expert_pins.lock().unwrap();
    pins.set_pin(&category, &agent_key);
    println!("📌 Expert pin: {} → {}", category, agent_key);
    format!("Pinned {} to {}", category, agent_key)
}

/// Phase 1 fallback: Routes a user intent using the heuristic keyword matcher.
#[tauri::command]
fn route_intent(intent: &str, state: State<AppState>) -> String {
    state.sam_logic.route_heuristic(intent)
}

/// Phase 2A: Two-stage LLM pipeline (Kimi Commander → Opus Audit Hook).
#[tauri::command]
async fn route_intent_live(intent: String, state: State<'_, AppState>) -> Result<String, String> {
    let sam_logic = state.sam_logic.clone();
    let mut result: PipelineResult = orchestrator::route_intent_live(&intent, &sam_logic).await;

    // Phase 3: Grandma-Speak Interceptor
    if let Some(e) = &result.error {
        let grandma_msg = orchestrator::translator::translate_error_to_grandma(e, &sam_logic).await;
        result.error = Some(format!("{}\n\n[Dev Details: {}]", grandma_msg, e));
    }

    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Phase 2B: Smart Mode dispatcher (Gemini Flash triage → optimal model).
#[tauri::command]
async fn dispatch_smart(intent: String, state: State<'_, AppState>) -> Result<String, String> {
    let sam_logic = state.sam_logic.clone();
    let mut result: DispatchResult = router::dispatch_smart(&intent, &sam_logic).await;

    // Phase 3: Grandma-Speak Interceptor
    if let Some(e) = &result.error {
        let grandma_msg = orchestrator::translator::translate_error_to_grandma(e, &sam_logic).await;

        // Phase 4: Sentinel auto-capture on Smart Mode errors
        state.sentinel.capture_error(
            "dispatch_error",
            "SMART_MODE_FAIL",
            e,
            &result.model_name,
            &result.routed_to,
            None,
        );

        result.error = Some(format!("{}\n\n[Dev Details: {}]", grandma_msg, e));
    }

    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Phase 2B: Expert Mode dispatcher (use pinned model for task category).
#[tauri::command]
fn dispatch_expert(
    intent: String,
    task_category: String,
    state: State<AppState>,
) -> Result<String, String> {
    let pins = state.expert_pins.lock().unwrap();
    let result = router::dispatch_expert(&intent, &task_category, &pins, &state.sam_logic);
    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Called by the frontend when the app wakes from sleep.
#[tauri::command]
fn resync_messages(state: State<AppState>) -> Vec<String> {
    let mut queue = state.pending_resync.lock().unwrap();
    queue.drain(..).collect()
}

/// Simulates queuing a message (for dev/testing).
#[tauri::command]
fn queue_message(message: String, state: State<AppState>) {
    let mut queue = state.pending_resync.lock().unwrap();
    queue.push(message);
}

// ──────────────────────────────────────────────────
// S-ION Sentinel IPC Commands
// ──────────────────────────────────────────────────

/// Returns the security log (egress pass/block events).
#[tauri::command]
fn get_security_log(state: State<AppState>) -> String {
    let egress = state.egress.lock().unwrap();
    let log = egress.get_log();
    serde_json::to_string(&log).unwrap_or_default()
}

/// Returns the oldest pending Sentinel report (for consent toast).
#[tauri::command]
fn get_pending_report(state: State<AppState>) -> String {
    match state.sentinel.get_pending_report() {
        Some(r) => serde_json::to_string(&r).unwrap_or_default(),
        None => "null".into(),
    }
}

/// User approved: send the report to Railway.
#[tauri::command]
async fn approve_report(state: State<'_, AppState>) -> Result<String, String> {
    state.sentinel.approve_and_send().await
}

/// User dismissed: discard the pending report.
#[tauri::command]
fn dismiss_report(state: State<AppState>) -> String {
    state.sentinel.dismiss_report();
    "Report dismissed".into()
}

/// Check if the current install is the founder/developer.
#[tauri::command]
fn is_founder(state: State<AppState>) -> bool {
    state.sentinel.is_founder()
}

// ──────────────────────────────────────────────────
// S-ION Sandbox IPC Commands (Phase 5)
// ──────────────────────────────────────────────────

/// Execute a script in the sandbox. Returns SandboxResult with diff + snapshot ID.
#[tauri::command]
fn sandbox_execute(
    script: String,
    agent_key: String,
    state: State<AppState>,
) -> Result<String, String> {
    let mut sandbox = state.sandbox.lock().unwrap();
    let result = sandbox.execute(&script, &agent_key)?;
    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Apply sandbox changes to a target directory on the host.
#[tauri::command]
fn sandbox_apply(
    execution_id: String,
    target_dir: String,
    state: State<AppState>,
) -> Result<String, String> {
    let sandbox = state.sandbox.lock().unwrap();
    let target = std::path::Path::new(&target_dir);
    let applied = sandbox.apply(&execution_id, target)?;
    Ok(format!("{} files applied to {}", applied, target_dir))
}

/// Snap-Back: restore the pre-execution snapshot. All changes disappear.
#[tauri::command]
fn sandbox_snapback(snapshot_id: String, state: State<AppState>) -> Result<String, String> {
    let sandbox = state.sandbox.lock().unwrap();
    sandbox.snap_back(&snapshot_id)?;
    Ok(format!(
        "Snap-Back complete: restored snapshot {}",
        &snapshot_id[..8]
    ))
}

/// Get sandbox execution history.
#[tauri::command]
fn sandbox_history(state: State<AppState>) -> String {
    let sandbox = state.sandbox.lock().unwrap();
    serde_json::to_string(&sandbox.get_history()).unwrap_or_default()
}

// ──────────────────────────────────────────────────
// Egress Filter IPC Commands
// ──────────────────────────────────────────────────

/// Validate a URL against the egress allowlist.
#[tauri::command]
fn validate_egress(
    url: String,
    agent_key: String,
    state: State<AppState>,
) -> Result<String, String> {
    let egress = state.egress.lock().unwrap();
    egress.validate(&url, &agent_key)?;
    Ok(format!("Egress pass: {}", url))
}

/// Add a user-defined domain to the egress allowlist at runtime.
#[tauri::command]
fn add_egress_domain(domain: String, state: State<AppState>) -> String {
    let mut egress = state.egress.lock().unwrap();
    egress.add_user_domain(&domain);
    format!("Added {} to egress allowlist", domain)
}

// ──────────────────────────────────────────────────
// Bridge Heartbeat IPC Commands
// ──────────────────────────────────────────────────

/// Initiate the Secret Handshake with the Railway bridge.
#[tauri::command]
async fn bridge_handshake(state: State<'_, AppState>) -> Result<String, String> {
    match state.heartbeat.handshake().await {
        Ok(true) => Ok("Bridge handshake: SUCCESS".into()),
        Ok(false) => Ok("Bridge offline (no URL configured)".into()),
        Err(e) => Err(e),
    }
}

/// Heartbeat pulse: check for and claim the next pending mission.
#[tauri::command]
async fn bridge_pulse(state: State<'_, AppState>) -> Result<String, String> {
    match state.heartbeat.pulse().await {
        Ok(Some(mission)) => serde_json::to_string(&mission).map_err(|e| e.to_string()),
        Ok(None) => Ok("null".into()),
        Err(e) => Err(e),
    }
}

/// Check how many missions are waiting on the bridge.
#[tauri::command]
async fn bridge_pending(state: State<'_, AppState>) -> Result<String, String> {
    let count = state.heartbeat.check_pending().await?;
    Ok(format!("{}", count))
}

/// Get locally cached missions.
#[tauri::command]
fn bridge_local_missions(state: State<AppState>) -> String {
    serde_json::to_string(&state.heartbeat.get_local_missions()).unwrap_or_default()
}

// ──────────────────────────────────────────────────
// Application Entry
// ──────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env file for API keys (Fortress Layer)
    match dotenvy::dotenv() {
        Ok(path) => println!("🔑 Loaded secrets from: {:?}", path),
        Err(e) => println!("⚠️  No .env file found ({}), heuristic fallback active", e),
    }

    // Load SAM_LOGIC.yaml at build time
    let sam_logic_yaml = include_str!("../SAM_LOGIC.yaml");
    let sam_logic: SamLogic =
        serde_yaml::from_str(sam_logic_yaml).expect("Failed to parse SAM_LOGIC.yaml");

    println!("🧠 S-ION Engine v{} initialized", sam_logic.version);
    println!(
        "🎯 Commander: {} ({})",
        sam_logic.swarm.commander.model, sam_logic.swarm.commander.designation
    );
    println!(
        "🛡️  Audit Hook: {} ({})",
        sam_logic.swarm.audit_hook.model, sam_logic.swarm.audit_hook.designation
    );
    println!(
        "🔍 Analyst: {} ({})",
        sam_logic.swarm.analyst.model, sam_logic.swarm.analyst.designation
    );
    println!(
        "👁️  Visionary: {} ({})",
        sam_logic.swarm.visionary.model, sam_logic.swarm.visionary.designation
    );
    println!(
        "🔨 Builder: {} ({})",
        sam_logic.swarm.builder.model, sam_logic.swarm.builder.designation
    );
    println!(
        "🏃 Scout: {} ({})",
        sam_logic.swarm.scout.model, sam_logic.swarm.scout.designation
    );
    println!(
        "🍌 Fast Designer: {} ({})",
        sam_logic.swarm.fast_designer.model, sam_logic.swarm.fast_designer.designation
    );
    println!(
        "🎨 Pro Designer: {} ({})",
        sam_logic.swarm.pro_designer.model, sam_logic.swarm.pro_designer.designation
    );
    println!("⚡ Smart Triage: {}", sam_logic.smart_mode.triage_model);

    // Initialize Expert Mode pins from YAML defaults
    let expert_pins = ExpertPins::from_yaml_defaults(&sam_logic.expert_mode.default_pins);

    // Initialize Egress Filter, Sentinel, Sandbox, and Bridge Heartbeat
    let egress = EgressFilter::from_sam_logic(&sam_logic);
    let sentinel = Sentinel::new(&sam_logic);
    let sandbox_config = SandboxConfig::default();
    println!(
        "🏗️  Sandbox config: memory_limit={}MB, timeout={}s, network={}",
        sandbox_config.memory_limit / (1024 * 1024),
        sandbox_config.timeout.as_secs(),
        if sandbox_config.network_enabled {
            "enabled"
        } else {
            "disabled (jailed)"
        }
    );
    let sandbox = Sandbox::new(sandbox_config);
    println!("🏗️  Sandbox backend: {}", sandbox.backend.label());
    let heartbeat = BridgeHeartbeat::new(&sam_logic.privacy.sentinel.railway_endpoint);

    let app_state = AppState {
        sam_logic,
        mode: Mutex::new(RuntimeMode::Smart),
        expert_pins: Mutex::new(expert_pins),
        pending_resync: Mutex::new(Vec::new()),
        egress: Mutex::new(egress),
        sentinel,
        sandbox: Mutex::new(sandbox),
        heartbeat,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_sam_logic,
            get_theme,
            get_mode,
            set_mode,
            get_model_pins,
            set_model_pin,
            route_intent,
            route_intent_live,
            dispatch_smart,
            dispatch_expert,
            resync_messages,
            queue_message,
            get_security_log,
            get_pending_report,
            approve_report,
            dismiss_report,
            is_founder,
            sandbox_execute,
            sandbox_apply,
            sandbox_snapback,
            sandbox_history,
            validate_egress,
            add_egress_domain,
            bridge_handshake,
            bridge_pulse,
            bridge_pending,
            bridge_local_missions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running S-ION");
}
