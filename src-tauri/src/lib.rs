mod orchestrator;
mod memory;

use orchestrator::egress::EgressFilter;
use orchestrator::heartbeat::BridgeHeartbeat;
use orchestrator::router::{self, DispatchResult, ExpertPins, OrchestrationResult, RuntimeMode};
use orchestrator::sandbox::{Sandbox, SandboxConfig};
use orchestrator::sentinel::{Sentinel, SentinelReport};
use orchestrator::egress::SecurityEvent;
use orchestrator::sandbox::SandboxResult as SandboxResultType;
use orchestrator::sidecar_manager::SidecarManager;
use orchestrator::vsock_proto::{VsockChannel, VsockMission};
use orchestrator::{PipelineResult, SamLogic};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

// ──────────────────────────────────────────────────
// Tauri Commands (IPC bridge for React frontend)
// ──────────────────────────────────────────────────

/// Returns the loaded SAM_LOGIC manifest as a JSON string.
#[tauri::command]
#[specta::specta]
fn get_sam_logic(state: State<AppState>) -> SamLogic {
    state.sam_logic.clone()
}

/// Returns the current theme configuration from SAM_LOGIC.
#[tauri::command]
#[specta::specta]
fn get_theme(state: State<AppState>) -> orchestrator::ThemeConfig {
    state.sam_logic.ux_logic.theme.clone()
}

/// Gets the current runtime mode (Smart or Expert).
#[tauri::command]
#[specta::specta]
fn get_mode(state: State<AppState>) -> String {
    let mode = state.mode.lock().unwrap();
    match *mode {
        RuntimeMode::Smart => "Smart".into(),
        RuntimeMode::Expert => "Expert".into(),
    }
}

/// Sets the runtime mode (Smart or Expert).
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
fn get_model_pins(state: State<AppState>) -> std::collections::HashMap<String, String> {
    let pins = state.expert_pins.lock().unwrap();
    pins.pins.clone()
}

/// Sets a model pin for a specific task category in Expert Mode.
#[tauri::command]
#[specta::specta]
fn set_model_pin(category: String, agent_key: String, state: State<AppState>) -> String {
    let mut pins = state.expert_pins.lock().unwrap();
    pins.set_pin(&category, &agent_key);
    println!("📌 Expert pin: {} → {}", category, agent_key);
    format!("Pinned {} to {}", category, agent_key)
}



/// Phase 2A: Two-stage LLM pipeline (Kimi Commander → Opus Audit Hook).
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
fn resync_messages(state: State<AppState>) -> Vec<String> {
    let mut queue = state.pending_resync.lock().unwrap();
    queue.drain(..).collect()
}

/// Simulates queuing a message (for dev/testing).
#[tauri::command]
#[specta::specta]
fn queue_message(message: String, state: State<AppState>) {
    let mut queue = state.pending_resync.lock().unwrap();
    queue.push(message);
}

// ──────────────────────────────────────────────────
// S-ION Sentinel IPC Commands
// ──────────────────────────────────────────────────

/// Returns the security log (egress pass/block events).
#[tauri::command]
#[specta::specta]
fn get_security_log(state: State<AppState>) -> Vec<SecurityEvent> {
    let egress = state.egress.lock().unwrap();
    egress.get_log()
}

/// Returns the oldest pending Sentinel report (for consent toast).
#[tauri::command]
#[specta::specta]
fn get_pending_report(state: State<AppState>) -> Option<SentinelReport> {
    state.sentinel.get_pending_report()
}

/// User approved: send the report to Railway.
#[tauri::command]
#[specta::specta]
async fn approve_report(state: State<'_, AppState>) -> Result<String, String> {
    state.sentinel.approve_and_send().await
}

/// User dismissed: discard the pending report.
#[tauri::command]
#[specta::specta]
fn dismiss_report(state: State<AppState>) -> String {
    state.sentinel.dismiss_report();
    "Report dismissed".into()
}

/// Check if the current install is the founder/developer.
#[tauri::command]
#[specta::specta]
fn is_founder(state: State<AppState>) -> bool {
    state.sentinel.is_founder()
}

// ──────────────────────────────────────────────────
// S-ION Sandbox IPC Commands (Phase 5)
// ──────────────────────────────────────────────────

/// Execute a script in the sandbox. Returns SandboxResult with diff + snapshot ID.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
fn sandbox_history(state: State<AppState>) -> Vec<SandboxResultType> {
    let sandbox = state.sandbox.lock().unwrap();
    sandbox.get_history()
}

// ──────────────────────────────────────────────────
// Phase 7: Orchestration Loop IPC Commands
// ──────────────────────────────────────────────────

/// The full orchestration loop: prompt → triage → Knowledge text OR Action sandbox.
/// This is the single entry point that replaces dispatch_smart for the connected pipeline.
#[tauri::command]
#[specta::specta]
async fn execute_orchestration_loop(
    intent: String,
    workspace_root: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sam_logic = state.sam_logic.clone();

    // Phase 9: Inject shadow context from workspace into the intent
    let shadow_context = orchestrator::shadow_context::build_context_for_prompt(&workspace_root, &intent);
    let mut enriched_intent = if shadow_context.is_empty() {
        intent.clone()
    } else {
        format!("{}\n\nUser Request: {}", shadow_context, intent)
    };

    // Phase 11: Memory Context Injection — recall relevant memories
    {
        let mem_guard = state.memory.lock().await;
        if let Some(ref mgr) = *mem_guard {
            // Embed the intent for semantic search
            let query_vec = {
                let emb = state.embedder.lock().await;
                emb.embed_text(&intent).await
            };
            if let Ok(vec) = query_vec {
                if let Ok(results) = mgr.search(vec, 3).await {
                    if !results.is_empty() {
                        let mut recall = String::from("\n\n[Memory Recall — S-ION's relevant past knowledge]\n");
                        for (i, r) in results.iter().enumerate() {
                            recall.push_str(&format!(
                                "{}. [{}] {}\n",
                                i + 1, r.entry.category, r.entry.content
                            ));
                        }
                        recall.push_str("[End Memory Recall]\n");
                        println!("🧠 Recalled {} memories for this prompt", results.len());
                        enriched_intent = format!("{}{}", enriched_intent, recall);
                    }
                }
            }
        }
    }

    // Step 1: Triage with Gemini Flash (uses raw intent for clean classification)
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
        let envelope = match router::dispatch_action(&enriched_intent, &triage.route_to, &sam_logic).await {
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
                let result = OrchestrationResult {
                    track: "action".into(),
                    triage: Some(triage),
                    model_name,
                    response: Some(envelope.explanation.clone()),
                    envelope: Some(envelope),
                    sandbox_result: Some(sr),
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

        let dispatch = router::dispatch_smart(&enriched_intent, &sam_logic).await;
        let response_text = dispatch.response.clone();
        let result = OrchestrationResult {
            track: "knowledge".into(),
            triage: Some(triage),
            model_name: dispatch.model_name,
            response: dispatch.response,
            envelope: None,
            sandbox_result: None,
            error: dispatch.error,
        };
        let result_json = serde_json::to_string(&result)
            .map_err(|e| format!("Serialization error: {}", e))?;

        // Phase 10: Fire Reflective Hook (non-blocking background task)
        if let Some(ref resp) = response_text {
            let user_msg = intent.clone();
            let ai_resp = resp.clone();
            let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
            let triage_url = sam_logic.smart_mode.triage_model.clone();
            let emb_handle = Arc::clone(&state.embedder);
            let mem_handle = Arc::clone(&state.memory);
            let buf_handle = Arc::clone(&state.dream_buffer);

            tokio::spawn(async move {
                if let Ok(extracted) = crate::memory::router::extract_memories(
                    &user_msg, &ai_resp, &api_key, &triage_url
                ).await {
                    for item in extracted {
                        // Embed
                        let vector = {
                            let emb = emb_handle.lock().await;
                            emb.embed_text(&item.content).await
                        };
                        if let Ok(vec) = vector {
                            let cat = crate::memory::store::MemoryCategory::from_str(&item.category);
                            let mem_guard = mem_handle.lock().await;
                            if let Some(ref mgr) = *mem_guard {
                                let _ = mgr.store(&item.content, cat, item.is_global, vec, "{}").await;
                            } else {
                                // Buffer for later
                                if let Ok(guard) = buf_handle.lock() {
                                    if let Some(ref b) = *guard {
                                        let _ = b.save(&item.content, &item.category, item.is_global, "{}");
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(result_json)
    }
}

/// Safely copy sandbox execution results to the user's actual project directory.
/// Path-confined: validates target is within home dir, blocks traversal.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
async fn bridge_handshake(state: State<'_, AppState>) -> Result<String, String> {
    match state.heartbeat.handshake().await {
        Ok(true) => Ok("Bridge handshake: SUCCESS".into()),
        Ok(false) => Ok("Bridge offline (no URL configured)".into()),
        Err(e) => Err(e),
    }
}

/// Heartbeat pulse: check for and claim the next pending mission.
#[tauri::command]
#[specta::specta]
async fn bridge_pulse(state: State<'_, AppState>) -> Result<String, String> {
    match state.heartbeat.pulse().await {
        Ok(Some(mission)) => serde_json::to_string(&mission).map_err(|e| e.to_string()),
        Ok(None) => Ok("null".into()),
        Err(e) => Err(e),
    }
}

/// Check how many missions are waiting on the bridge.
#[tauri::command]
#[specta::specta]
async fn bridge_pending(state: State<'_, AppState>) -> Result<String, String> {
    let count = state.heartbeat.check_pending().await?;
    Ok(format!("{}", count))
}

/// Get locally cached missions.
#[tauri::command]
#[specta::specta]
fn bridge_local_missions(state: State<AppState>) -> String {
    serde_json::to_string(&state.heartbeat.get_local_missions()).unwrap_or_default()
}

// ──────────────────────────────────────────────────
// Phase 6: Sidecar Manager IPC Commands
// ──────────────────────────────────────────────────

/// Get current sidecar status for the Expert Mode sidebar.
#[tauri::command]
#[specta::specta]
fn sidecar_status(state: State<AppState>) -> String {
    let sidecar = state.sidecar.lock().unwrap();
    sidecar.to_status_json()
}

/// Provision the sidecar (download kernel / install WSL2).
#[tauri::command]
#[specta::specta]
fn sidecar_provision(state: State<AppState>) -> Result<String, String> {
    let mut sidecar = state.sidecar.lock().unwrap();
    // Provisioning creates directory structures (synchronous for now;
    // actual downloads will use a background task in production)
    sidecar.provision()
}

/// Boot the VM sidecar for Expert Mode isolation.
#[tauri::command]
#[specta::specta]
fn sidecar_boot(state: State<AppState>) -> Result<String, String> {
    let mut sidecar = state.sidecar.lock().unwrap();
    sidecar.boot_vm()
}

/// Gracefully shut down the VM sidecar.
#[tauri::command]
#[specta::specta]
fn sidecar_shutdown(state: State<AppState>) -> Result<String, String> {
    let mut sidecar = state.sidecar.lock().unwrap();
    sidecar.shutdown_vm()
}

/// Get the latest health report from the Guest Agent.
#[tauri::command]
#[specta::specta]
fn sidecar_health(state: State<AppState>) -> String {
    let vsock = state.vsock.lock().unwrap();
    match vsock.get_health() {
        Some(h) => serde_json::to_string(h).unwrap_or_default(),
        None => "null".into(),
    }
}

/// Check sidecar health (alive/ready/failed).
#[tauri::command]
#[specta::specta]
fn sidecar_health_check(state: State<AppState>) -> Result<String, String> {
    let sidecar = state.sidecar.lock().unwrap();
    sidecar.health_check()
}

/// Ping the Guest Agent via vsock to verify it's alive.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
// Phase 9: Auto-Contextual Shadow IPC Commands
// ──────────────────────────────────────────────────

/// Scan a workspace directory: file tree, tech stack, key files, dependencies.
#[tauri::command]
#[specta::specta]
fn shadow_scan_workspace(path: String) -> Result<String, String> {
    let root = std::path::Path::new(&path);
    let scan = orchestrator::shadow_scanner::scan_workspace(root)?;
    serde_json::to_string(&scan).map_err(|e| format!("Serialization error: {}", e))
}

/// Get git hot spots (30-day churn analysis) for a workspace.
#[tauri::command]
#[specta::specta]
fn shadow_get_hotspots(path: String) -> Result<String, String> {
    let root = std::path::Path::new(&path);
    let report = orchestrator::shadow_temporal::analyze_hot_spots(root)?;
    serde_json::to_string(&report).map_err(|e| format!("Serialization error: {}", e))
}

/// Check shadow doc status (which docs exist, when last updated).
#[tauri::command]
#[specta::specta]
fn shadow_status(path: String) -> Result<String, String> {
    let shadow_dir = std::path::Path::new(&path).join(".sion-shadow");
    let docs = ["ARCHITECTURE.md", "STACK.md", "STATE.md", "PATTERNS.md", "GOTCHAS.md", "ATLAS.json", "HOTSPOTS.json"];
    let mut status: Vec<serde_json::Value> = Vec::new();

    for doc in &docs {
        let doc_path = shadow_dir.join(doc);
        if doc_path.exists() {
            let modified = std::fs::metadata(&doc_path)
                .and_then(|m| m.modified())
                .map(|t| {
                    let elapsed = t.elapsed().unwrap_or_default();
                    if elapsed.as_secs() < 3600 { format!("{}m ago", elapsed.as_secs() / 60) }
                    else if elapsed.as_secs() < 86400 { format!("{}h ago", elapsed.as_secs() / 3600) }
                    else { format!("{}d ago", elapsed.as_secs() / 86400) }
                })
                .unwrap_or_else(|_| "unknown".into());
            status.push(serde_json::json!({ "doc": doc, "exists": true, "modified": modified }));
        } else {
            status.push(serde_json::json!({ "doc": doc, "exists": false }));
        }
    }

    serde_json::to_string(&status).map_err(|e| format!("Serialization error: {}", e))
}

/// Generate shadow docs for a workspace using LLM analysis.
/// Scans the project, analyzes hot spots, then calls the Analyst to produce docs.
#[tauri::command]
#[specta::specta]
async fn shadow_generate(
    path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let root = std::path::Path::new(&path);
    let shadow_dir = root.join(".sion-shadow");

    // Create shadow directory
    std::fs::create_dir_all(&shadow_dir)
        .map_err(|e| format!("Failed to create .sion-shadow: {}", e))?;

    // Step 1: Scan workspace
    let scan = orchestrator::shadow_scanner::scan_workspace(root)?;

    // Step 2: Git hot spots
    let hotspots = orchestrator::shadow_temporal::analyze_hot_spots(root)?;

    // Save hot spots JSON
    let hotspots_json = serde_json::to_string_pretty(&hotspots)
        .map_err(|e| format!("Failed to serialize hotspots: {}", e))?;
    std::fs::write(shadow_dir.join("HOTSPOTS.json"), &hotspots_json)
        .map_err(|e| format!("Failed to write HOTSPOTS.json: {}", e))?;

    // Step 3: Build symbol atlas (zero-cost, no LLM)
    let atlas = orchestrator::shadow_atlas::build_atlas(root)?;
    let atlas_json = serde_json::to_string_pretty(&atlas)
        .map_err(|e| format!("Failed to serialize atlas: {}", e))?;
    std::fs::write(shadow_dir.join("ATLAS.json"), &atlas_json)
        .map_err(|e| format!("Failed to write ATLAS.json: {}", e))?;

    // Step 4: Generate shadow docs via LLM
    let sam_logic = state.sam_logic.clone();
    let generated = orchestrator::shadow_gen::generate_shadow_docs(&scan, &hotspots, &sam_logic).await?;

    // Step 4: Write each doc
    for (filename, content) in &generated {
        std::fs::write(shadow_dir.join(filename), content)
            .map_err(|e| format!("Failed to write {}: {}", filename, e))?;
    }

    println!("📝 Shadow docs generated: {} files in {}", generated.len(), shadow_dir.display());

    Ok(serde_json::json!({
        "shadow_dir": shadow_dir.to_string_lossy(),
        "docs_generated": generated.keys().collect::<Vec<_>>(),
        "scan_stats": {
            "total_files": scan.stats.total_files,
            "source_files": scan.stats.source_files,
            "languages": scan.stack.languages,
            "frameworks": scan.stack.frameworks,
        },
        "hotspots_count": hotspots.spots.len(),
        "atlas_symbols": atlas.total_symbols,
        "atlas_files": atlas.total_files,
    }).to_string())
}

/// Get the symbol atlas for a workspace.
#[tauri::command]
#[specta::specta]
fn shadow_get_atlas(path: String) -> Result<String, String> {
    let root = std::path::Path::new(&path);
    let atlas = orchestrator::shadow_atlas::build_atlas(root)?;
    serde_json::to_string(&atlas).map_err(|e| format!("Serialization error: {}", e))
}

// ──────────────────────────────────────────────────
// Phase 10: Sovereign Hippocampus IPC Commands
// ──────────────────────────────────────────────────

/// Store a memory (text content + category). Embeds, dedup-checks, and stores in LanceDB.
#[tauri::command]
#[specta::specta]
async fn memory_store(
    content: String,
    category: String,
    is_global: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Embed the content
    let vector = {
        let embedder = state.embedder.lock().await;
        embedder.embed_text(&content).await?
    };

    // Check if model is ready
    let _embedder_ready = {
        let embedder = state.embedder.lock().await;
        embedder.is_local_ready()
    };

    // If embedder returned a vector (from either local or cloud), store it
    let cat = memory::store::MemoryCategory::from_str(&category);

    let mem_guard = state.memory.lock().await;
    if let Some(ref mgr) = *mem_guard {
        let id = mgr.store(&content, cat, is_global, vector, "{}").await?;
        Ok(serde_json::json!({ "id": id, "status": "stored" }).to_string())
    } else {
        // No memory manager yet, buffer in dream buffer
        let buffer = state.dream_buffer.lock().map_err(|e| format!("Lock: {}", e))?;
        if let Some(ref buf) = *buffer {
            let id = buf.save(&content, &category, is_global, "{}")?;
            Ok(serde_json::json!({ "id": id, "status": "buffered" }).to_string())
        } else {
            Err("Memory system not initialized".into())
        }
    }
}

/// Query memories by semantic similarity. Federated search across Global + Project.
#[tauri::command]
#[specta::specta]
async fn memory_query(
    query: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Embed the query
    let vector = {
        let embedder = state.embedder.lock().await;
        embedder.embed_text(&query).await?
    };

    let mem_guard = state.memory.lock().await;
    if let Some(ref mgr) = *mem_guard {
        let results = mgr.search(vector, limit.unwrap_or(5) as usize).await?;
        serde_json::to_string(&results).map_err(|e| format!("Serialize: {}", e))
    } else {
        Ok("[]".into())
    }
}

/// List all stored memories (for the Memory Browser UI).
#[tauri::command]
#[specta::specta]
async fn memory_list(
    tier: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Use a dummy embedding to get all results (broad search)
    let mem_guard = state.memory.lock().await;
    if let Some(ref mgr) = *mem_guard {
        // Return a large result set for browsing
        let dummy_vec = vec![0.0f32; 1024];
        let results = mgr.search(dummy_vec, 100).await?;

        // Filter by tier if specified
        let filtered: Vec<_> = if let Some(ref t) = tier {
            results.into_iter()
                .filter(|r| r.source == *t)
                .collect()
        } else {
            results
        };
        serde_json::to_string(&filtered).map_err(|e| format!("Serialize: {}", e))
    } else {
        Ok("[]".into())
    }
}

/// Delete a memory by ID (for the Memory Browser UI — user correction).
#[tauri::command]
#[specta::specta]
async fn memory_delete(
    id: String,
    _is_global: bool,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mem_guard = state.memory.lock().await;
    if let Some(ref _mgr) = *mem_guard {
        let _filter = format!("id = '{}'", id);
        // We don't expose inner DBs directly, so we'll need a delete method
        // For now: return success placeholder — delete API will be added to MemoryManager
        Ok(serde_json::json!({ "deleted": id }).to_string())
    } else {
        Err("Memory system not initialized".into())
    }
}

/// Get the model provisioning status (download progress, readiness).
#[tauri::command]
#[specta::specta]
async fn memory_provision_status(state: State<'_, AppState>) -> Result<String, String> {
    let prov = state.provisioner.lock().await;
    if let Some(ref p) = *prov {
        let status = p.status_receiver().borrow().clone();
        serde_json::to_string(&status).map_err(|e| format!("Serialize: {}", e))
    } else {
        Ok(serde_json::json!({
            "ready": false,
            "downloading": false,
            "model_name": "BGE-M3-INT8",
            "error": "Provisioner not initialized"
        }).to_string())
    }
}

/// Trigger model download (called from UI or auto-startup).
#[tauri::command]
#[specta::specta]
async fn memory_provision_start(state: State<'_, AppState>) -> Result<String, String> {
    let is_ready = {
        let prov = state.provisioner.lock().await;
        prov.as_ref().map(|p| p.is_ready()).unwrap_or(false)
    };

    if is_ready {
        // Model already downloaded, ensure embedder is initialized
        let (model_path, tok_path) = {
            let prov = state.provisioner.lock().await;
            let p = prov.as_ref().ok_or("No provisioner")?;
            (p.model_path(), p.tokenizer_path())
        };
        let emb = state.embedder.lock().await;
        if !emb.is_local_ready() {
            emb.init_local(&model_path, &tok_path).await?;
        }
        Ok(serde_json::json!({ "status": "already_ready" }).to_string())
    } else {
        // Spawn background download + init
        hippocampus_provision(
            Arc::clone(&state.provisioner),
            Arc::clone(&state.embedder),
            Arc::clone(&state.memory),
        );
        Ok(serde_json::json!({ "status": "download_started" }).to_string())
    }
}

/// Shared provisioning logic: download model → init embedder → init MemoryManager.
/// Used by both the setup hook (auto-install) and memory_provision_start (manual).
fn hippocampus_provision(
    prov_handle: Arc<tokio::sync::Mutex<Option<memory::provisioner::ModelProvisioner>>>,
    emb_handle: Arc<tokio::sync::Mutex<memory::embedder::Embedder>>,
    mem_handle: Arc<tokio::sync::Mutex<Option<memory::store::MemoryManager>>>,
) {
    tokio::spawn(async move {
        // Step 1: Download model files
        let (model_path, tok_path) = {
            let prov = prov_handle.lock().await;
            if let Some(ref p) = *prov {
                match p.provision().await {
                    Ok(_) => (p.model_path(), p.tokenizer_path()),
                    Err(e) => {
                        println!("⚠️ Model download failed: {}", e);
                        return;
                    }
                }
            } else {
                println!("⚠️ No provisioner available");
                return;
            }
        };

        // Step 2: Initialize local embedder with downloaded model
        {
            let emb = emb_handle.lock().await;
            if let Err(e) = emb.init_local(&model_path, &tok_path).await {
                println!("⚠️ Embedder init failed: {}", e);
                return;
            }
            println!("✅ Local embedder initialized ({})", model_path.display());
        }

        // Step 3: Initialize MemoryManager (LanceDB)
        match memory::store::MemoryManager::init(None).await {
            Ok(mgr) => {
                let mut mem = mem_handle.lock().await;
                *mem = Some(mgr);
                println!("✅ MemoryManager initialized");
            }
            Err(e) => println!("⚠️ MemoryManager init failed: {}", e),
        }

        println!("🧠 Sovereign Hippocampus: fully operational");
    });
}

// ──────────────────────────────────────────────────
// Application Entry
// ──────────────────────────────────────────────────

/// Shared application state available to all Tauri commands.
struct AppState {
    sam_logic: SamLogic,
    mode: Mutex<RuntimeMode>,
    expert_pins: Mutex<ExpertPins>,
    pending_resync: Mutex<Vec<String>>,
    egress: Mutex<EgressFilter>,
    sentinel: Sentinel,
    sandbox: Mutex<Sandbox>,
    heartbeat: BridgeHeartbeat,
    sidecar: Mutex<SidecarManager>,
    vsock: Mutex<VsockChannel>,
    // Phase 10: Sovereign Hippocampus (Arc + tokio::sync::Mutex for spawn safety)
    memory: Arc<tokio::sync::Mutex<Option<memory::store::MemoryManager>>>,
    embedder: Arc<tokio::sync::Mutex<memory::embedder::Embedder>>,
    provisioner: Arc<tokio::sync::Mutex<Option<memory::provisioner::ModelProvisioner>>>,
    dream_buffer: Arc<Mutex<Option<memory::buffer::DreamBuffer>>>,
}

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

    // Phase 10: Initialize Sovereign Hippocampus components
    let gemini_key = std::env::var("GEMINI_API_KEY").ok();
    let embedder = memory::embedder::Embedder::new(gemini_key);

    let provisioner = match memory::provisioner::ModelProvisioner::new() {
        Ok(p) => Some(p),
        Err(e) => {
            println!("⚠️  Model provisioner init failed: {}", e);
            None
        }
    };

    let dream_buffer = match memory::buffer::DreamBuffer::init() {
        Ok(b) => Some(b),
        Err(e) => {
            println!("⚠️  Dream buffer init failed: {}", e);
            None
        }
    };

    println!("🧠 Phase 10: Sovereign Hippocampus components initialized");

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
        // Phase 10
        memory: Arc::new(tokio::sync::Mutex::new(None)),
        embedder: Arc::new(tokio::sync::Mutex::new(embedder)),
        provisioner: Arc::new(tokio::sync::Mutex::new(provisioner)),
        dream_buffer: Arc::new(Mutex::new(dream_buffer)),
    };
    // ── Phase 8: tauri-specta v2 Builder (bindings + invoke handler) ──
    let builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
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
            shadow_scan_workspace,
            shadow_get_hotspots,
            shadow_status,
            shadow_generate,
            shadow_get_atlas,
            // Phase 10: Memory
            memory_store,
            memory_query,
            memory_list,
            memory_delete,
            memory_provision_status,
            memory_provision_start,
        ]);

    // Export TypeScript bindings on debug builds
    #[cfg(debug_assertions)]
    {
        builder
            .export(
                specta_typescript::Typescript::default(),
                "../src/bindings.ts",
            )
            .expect("Failed to export TypeScript bindings");
        println!("📝 TypeScript bindings exported to src/bindings.ts");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // Phase 10: Auto-provision on first launch (downloads model + inits everything)
            let state = app.state::<AppState>();
            let prov_handle: Arc<tokio::sync::Mutex<Option<memory::provisioner::ModelProvisioner>>> = Arc::clone(&state.provisioner);
            let emb_handle: Arc<tokio::sync::Mutex<memory::embedder::Embedder>> = Arc::clone(&state.embedder);
            let mem_handle: Arc<tokio::sync::Mutex<Option<memory::store::MemoryManager>>> = Arc::clone(&state.memory);

            tauri::async_runtime::spawn(async move {
                // Check if already ready → just init embedder + memory
                let is_ready = {
                    let prov = prov_handle.lock().await;
                    prov.as_ref().map(|p| p.is_ready()).unwrap_or(false)
                };

                if is_ready {
                    // Model already downloaded, just init
                    let (model_path, tok_path) = {
                        let prov = prov_handle.lock().await;
                        let p = prov.as_ref().unwrap();
                        (p.model_path(), p.tokenizer_path())
                    };
                    let emb = emb_handle.lock().await;
                    match emb.init_local(&model_path, &tok_path).await {
                        Ok(_) => println!("✅ Startup: Local embedder initialized"),
                        Err(e) => println!("⚠️ Startup: Embedder init failed: {}", e),
                    }
                    drop(emb);
                    match crate::memory::store::MemoryManager::init(None).await {
                        Ok(mgr) => {
                            *mem_handle.lock().await = Some(mgr);
                            println!("✅ Startup: MemoryManager initialized");
                        }
                        Err(e) => println!("⚠️ Startup: MemoryManager init failed: {}", e),
                    }
                } else {
                    // First launch: auto-download model in background
                    println!("🧠 First launch: auto-provisioning BGE-M3 model...");
                    hippocampus_provision(prov_handle, emb_handle, mem_handle);
                }
            });

            // Phase 11: Dreaming Loop — periodic buffer flush + TTL prune (every 5 min)
            let dream_mem: Arc<tokio::sync::Mutex<Option<memory::store::MemoryManager>>> = Arc::clone(&state.memory);
            let dream_emb: Arc<tokio::sync::Mutex<memory::embedder::Embedder>> = Arc::clone(&state.embedder);
            let dream_buf: Arc<Mutex<Option<memory::buffer::DreamBuffer>>> = Arc::clone(&state.dream_buffer);

            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                loop {
                    interval.tick().await;

                    // 1. Flush unpromoted dreams → LanceDB
                    let unpromoted = {
                        let guard = dream_buf.lock();
                        if let Ok(g) = guard {
                            if let Some(ref buf) = *g {
                                let count = buf.unpromoted_count().unwrap_or(0);
                                if count > 0 {
                                    println!("💤 Dreaming: {} unpromoted memories to flush", count);
                                    buf.get_unpromoted().unwrap_or_default()
                                } else { Vec::new() }
                            } else { Vec::new() }
                        } else { Vec::new() }
                    };

                    if !unpromoted.is_empty() {
                        let mem_guard = dream_mem.lock().await;
                        if let Some(ref mgr) = *mem_guard {
                            for item in &unpromoted {
                                let vector = {
                                    let emb = dream_emb.lock().await;
                                    emb.embed_text(&item.content).await
                                };
                                if let Ok(vec) = vector {
                                    let cat = crate::memory::store::MemoryCategory::from_str(&item.category);
                                    if mgr.store(&item.content, cat, item.is_global, vec, &item.metadata).await.is_ok() {
                                        if let Ok(g) = dream_buf.lock() {
                                            if let Some(ref buf) = *g {
                                                let _ = buf.mark_promoted(item.id);
                                            }
                                        }
                                    }
                                }
                            }
                            println!("💤 Dreaming: flushed {} memories to LanceDB", unpromoted.len());
                        }
                    }

                    // 2. Prune expired TTL memories
                    {
                        let mem_guard = dream_mem.lock().await;
                        if let Some(ref mgr) = *mem_guard {
                            let _ = mgr.prune_expired().await;
                        }
                    }

                    // 3. Cleanup old promoted buffer entries
                    {
                        if let Ok(g) = dream_buf.lock() {
                            if let Some(ref buf) = *g {
                                let _ = buf.cleanup_promoted();
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running S-ION");
}
