mod orchestrator;

use orchestrator::egress::EgressFilter;
use orchestrator::heartbeat::BridgeHeartbeat;
use orchestrator::router::{self, DispatchResult, ExpertPins, OrchestrationResult, RuntimeMode};
use orchestrator::sandbox::{Sandbox, SandboxConfig};
use orchestrator::sentinel::{Sentinel, SentinelReport};
use orchestrator::sidecar_manager::SidecarManager;
use orchestrator::vsock_proto::{VsockChannel, VsockMission};
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
    /// Sidecar Manager: VM lifecycle for Expert Mode isolation.
    sidecar: Mutex<SidecarManager>,
    /// Vsock Channel: framed communication to Guest Agent.
    vsock: Mutex<VsockChannel>,
}

// ──────────────────────────────────────────────────
// Tauri Commands (IPC bridge for React frontend)
// ──────────────────────────────────────────────────

/// Returns the loaded SAM_LOGIC manifest as a JSON string.
#[tauri::command]
fn get_sam_logic(state: State<AppState>) -> SamLogic {
    state.sam_logic.clone()
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
    match *mode {
        RuntimeMode::Smart => "Smart".into(),
        RuntimeMode::Expert => "Expert".into(),
    }
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
fn get_model_pins(state: State<AppState>) -> std::collections::HashMap<String, String> {
    let pins = state.expert_pins.lock().unwrap();
    pins.pins.clone()
}

/// Sets a model pin for a specific task category in Expert Mode.
#[tauri::command]
fn set_model_pin(category: String, agent_key: String, state: State<AppState>) -> String {
    let mut pins = state.expert_pins.lock().unwrap();
    pins.set_pin(&category, &agent_key);
    println!("📌 Expert pin: {} → {}", category, agent_key);
    format!("Pinned {} to {}", category, agent_key)
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
fn get_security_log(state: State<AppState>) -> Vec<serde_json::Value> {
    let egress = state.egress.lock().unwrap();
    let log = egress.get_log();
    log.into_iter()
        .map(|e| serde_json::to_value(e).unwrap_or_default())
        .collect()
}

/// Returns the oldest pending Sentinel report (for consent toast).
#[tauri::command]
fn get_pending_report(state: State<AppState>) -> Option<SentinelReport> {
    state.sentinel.get_pending_report()
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
fn sandbox_history(state: State<AppState>) -> Vec<serde_json::Value> {
    let sandbox = state.sandbox.lock().unwrap();
    sandbox.get_history().into_iter()
        .map(|r| serde_json::to_value(r).unwrap_or_default())
        .collect()
}

// ──────────────────────────────────────────────────
// Phase 7: Orchestration Loop IPC Commands
// ──────────────────────────────────────────────────

/// The full orchestration loop: prompt → triage → Knowledge text OR Action sandbox.
/// This is the single entry point that replaces dispatch_smart for the connected pipeline.
#[tauri::command]
async fn execute_orchestration_loop(
    intent: String,
    _workspace_root: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sam_logic = state.sam_logic.clone();

    // Step 1: Triage with Gemini Flash
    let triage = match router::call_gemini_flash_triage(&intent, &sam_logic).await {
        Ok(t) => t,
        Err(e) => {
            println!("⚠️  Triage failed, defaulting to knowledge track: {}", e);
            router::TriageResult {
                category: "simple_qa".into(),
                route_to: "analyst".into(),
                reasoning: format!("Triage fallback: {}", e),
                confidence: 0.0,
            }
        }
    };

    // Step 2: Decide track based on triage category
    let is_action_track = matches!(
        triage.category.as_str(),
        "deep_code" | "parallel_ui" | "code_generation" | "refactor"
    );

    let (_, model_name, _) = router::resolve_agent_public(&triage.route_to, &sam_logic);

    if is_action_track {
        // ── ACTION TRACK ──
        println!("🎯 Action Track: {} → sandbox execution", triage.category);

        // Step 3: Get ActionEnvelope from LLM (JSON mode)
        let envelope = match router::dispatch_action(&intent, &triage.route_to, &sam_logic).await {
            Ok(env) => env,
            Err(e) => {
                let result = OrchestrationResult {
                    track: "action".into(),
                    triage: Some(triage),
                    model_name,
                    response: None,
                    envelope: None,
                    sandbox_result: None,
                    error: Some(format!("ActionEnvelope generation failed: {}", e)),
                };
                return serde_json::to_string(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        };

        // Step 4: Deterministic audit
        if let Err(blocked_reason) = router::audit_envelope(&envelope) {
            let result = OrchestrationResult {
                track: "action".into(),
                triage: Some(triage),
                model_name,
                response: None,
                envelope: Some(envelope),
                sandbox_result: None,
                error: Some(blocked_reason),
            };
            return serde_json::to_string(&result)
                .map_err(|e| format!("Serialization error: {}", e));
        }

        // Step 5: Execute in sandbox
        let script = envelope.bash_commands.join(" && ");
        let sandbox_result = {
            let mut sandbox = state.sandbox.lock().unwrap();
            sandbox.execute(&script, &triage.route_to)
        };

        match sandbox_result {
            Ok(sr) => {
                let sr_json = serde_json::to_value(&sr).unwrap_or_default();
                let result = OrchestrationResult {
                    track: "action".into(),
                    triage: Some(triage),
                    model_name,
                    response: Some(envelope.explanation.clone()),
                    envelope: Some(envelope),
                    sandbox_result: Some(sr_json),
                    error: None,
                };
                serde_json::to_string(&result)
                    .map_err(|e| format!("Serialization error: {}", e))
            }
            Err(e) => {
                let result = OrchestrationResult {
                    track: "action".into(),
                    triage: Some(triage),
                    model_name,
                    response: None,
                    envelope: Some(envelope),
                    sandbox_result: None,
                    error: Some(format!("Sandbox execution failed: {}", e)),
                };
                serde_json::to_string(&result)
                    .map_err(|e| format!("Serialization error: {}", e))
            }
        }
    } else {
        // ── KNOWLEDGE TRACK ──
        println!("📚 Knowledge Track: {} → text response", triage.category);

        let dispatch = router::dispatch_smart(&intent, &sam_logic).await;
        let result = OrchestrationResult {
            track: "knowledge".into(),
            triage: Some(triage),
            model_name: dispatch.model_name,
            response: dispatch.response,
            envelope: None,
            sandbox_result: None,
            error: dispatch.error,
        };
        serde_json::to_string(&result)
            .map_err(|e| format!("Serialization error: {}", e))
    }
}

/// Safely copy sandbox execution results to the user's actual project directory.
/// Path-confined: validates target is within home dir, blocks traversal.
#[tauri::command]
fn sync_to_host(
    execution_id: String,
    workspace_root: String,
    state: State<AppState>,
) -> Result<String, String> {
    let sandbox = state.sandbox.lock().unwrap();
    let target = std::path::Path::new(&workspace_root);
    let applied = sandbox.apply(&execution_id, target)?;
    Ok(serde_json::json!({
        "applied": applied,
        "workspace": workspace_root
    }).to_string())
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
// Phase 6: Sidecar Manager IPC Commands
// ──────────────────────────────────────────────────

/// Get current sidecar status for the Expert Mode sidebar.
#[tauri::command]
fn sidecar_status(state: State<AppState>) -> String {
    let sidecar = state.sidecar.lock().unwrap();
    sidecar.to_status_json()
}

/// Provision the sidecar (download kernel / install WSL2).
#[tauri::command]
fn sidecar_provision(state: State<AppState>) -> Result<String, String> {
    let mut sidecar = state.sidecar.lock().unwrap();
    // Provisioning creates directory structures (synchronous for now;
    // actual downloads will use a background task in production)
    sidecar.provision()
}

/// Boot the VM sidecar for Expert Mode isolation.
#[tauri::command]
fn sidecar_boot(state: State<AppState>) -> Result<String, String> {
    let mut sidecar = state.sidecar.lock().unwrap();
    sidecar.boot_vm()
}

/// Gracefully shut down the VM sidecar.
#[tauri::command]
fn sidecar_shutdown(state: State<AppState>) -> Result<String, String> {
    let mut sidecar = state.sidecar.lock().unwrap();
    sidecar.shutdown_vm()
}

/// Get the latest health report from the Guest Agent.
#[tauri::command]
fn sidecar_health(state: State<AppState>) -> String {
    let vsock = state.vsock.lock().unwrap();
    match vsock.get_health() {
        Some(h) => serde_json::to_string(h).unwrap_or_default(),
        None => "null".into(),
    }
}

/// Check sidecar health (alive/ready/failed).
#[tauri::command]
fn sidecar_health_check(state: State<AppState>) -> Result<String, String> {
    let sidecar = state.sidecar.lock().unwrap();
    sidecar.health_check()
}

/// Ping the Guest Agent via vsock to verify it's alive.
#[tauri::command]
async fn vsock_ping(state: State<'_, AppState>) -> Result<String, String> {
    let port = {
        let vsock = state.vsock.lock().unwrap();
        vsock.port
    };
    let mut channel = VsockChannel::new();
    channel.port = port;
    let pong = channel.ping().await?;
    serde_json::to_string(&pong).map_err(|e| format!("Serialization error: {}", e))
}

/// Send a mission to the Guest Agent via vsock and get the result.
#[tauri::command]
async fn vsock_send_mission(command: String, state: State<'_, AppState>) -> Result<String, String> {
    let port = {
        let vsock = state.vsock.lock().unwrap();
        vsock.port
    };
    let mission = VsockMission::new(uuid::Uuid::new_v4().to_string(), command)
        .with_files(std::collections::HashMap::new())
        .with_timeout(30);
    let mut channel = VsockChannel::new();
    channel.port = port;
    let result = channel.send_mission(mission).await?;
    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Execute a bridge mission inside the sandbox.
/// Pull from Railway → dispatch → sandbox → Action Card.
#[tauri::command]
async fn bridge_execute_mission(state: State<'_, AppState>) -> Result<String, String> {
    // Step 1: Pull the next mission from the bridge
    let mission = state.heartbeat.pulse().await?;
    let mission = match mission {
        Some(m) => m,
        None => return Ok("{\"status\": \"no_missions\"}".into()),
    };

    // Step 2: Route the mission intent through Smart Mode
    let sam_logic = state.sam_logic.clone();
    let intent = mission.payload.as_deref().unwrap_or("(empty mission)");
    let dispatch = router::dispatch_smart(intent, &sam_logic).await;

    // Step 3: If the dispatch produced code, execute it in the sandbox
    if let Some(ref response) = dispatch.response {
        let sandbox_result = {
            let mut sandbox = state.sandbox.lock().unwrap();
            sandbox.execute(response, &dispatch.routed_to)
        };
        return serde_json::to_string(&sandbox_result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // No code to execute, just return the dispatch result
    serde_json::to_string(&dispatch).map_err(|e| format!("Serialization error: {}", e))
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

    // Initialize Phase 6: Sidecar Manager + Vsock Channel
    let sidecar = SidecarManager::detect();
    let vsock = VsockChannel::new();

    let app_state = AppState {
        sam_logic,
        mode: Mutex::new(RuntimeMode::Smart),
        expert_pins: Mutex::new(expert_pins),
        pending_resync: Mutex::new(Vec::new()),
        egress: Mutex::new(egress),
        sentinel,
        sandbox: Mutex::new(sandbox),
        heartbeat,
        sidecar: Mutex::new(sidecar),
        vsock: Mutex::new(vsock),
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
            sidecar_status,
            sidecar_provision,
            sidecar_boot,
            sidecar_shutdown,
            sidecar_health,
            sidecar_health_check,
            bridge_execute_mission,
            vsock_ping,
            vsock_send_mission,
            execute_orchestration_loop,
            sync_to_host,
        ])
        .run(tauri::generate_context!())
        .expect("error while running S-ION");
}
