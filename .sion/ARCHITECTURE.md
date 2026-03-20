# S-ION Architecture

## Philosophy: The Universal AI Browser

S-ION is a **Tauri 2.0** desktop application that acts as a browser for AI models. Instead of tabs for websites, users get **swarm agents** that coordinate via a central intelligence manifest (`SAM_LOGIC.yaml`).

## Two-Track Pipeline

Every user intent follows one of two tracks:

```
User Intent → Gemini Flash Triage
                 ├── Knowledge Track: Gemini/Kimi text response → rendered in UI
                 └── Action Track:    Commander plan → Sandbox execution → Action Card diff
```

### Smart Mode (default)
- **Triage**: Gemini Flash categorizes the intent
- **Routing**: Routes to the optimal agent (analyst, visionary, builder, etc.)
- **Output**: Text response or sandbox execution result

### Expert Mode
- User manually **pins** specific models to task categories
- Bypasses triage, dispatches directly to the pinned agent

## 8-Agent Swarm Roster

| Agent | Role | Default Model |
|-------|------|---------------|
| Commander | Execution planning | Kimi K2.5 |
| Audit Hook | Constitution enforcement | Kimi K2 |
| Analyst | Research & analysis | Gemini Flash |
| Visionary | Creative solutions | Gemini Pro |
| Builder | Code generation | Kimi K2.5 |
| Scout | Quick lookups | Gemini Flash |
| Fast Designer | Rapid UI prototyping | GPT-4o-mini |
| Pro Designer | Polished design | GPT-4o |

## IPC Architecture (48 Commands)

All frontend↔backend communication uses **Tauri IPC commands**. Every command is:
- Annotated with `#[specta::specta]` for type-safe TypeScript bindings
- Registered via `tauri_specta::Builder` (replaces `generate_handler!`)
- Auto-exported to `src/bindings.ts` during debug builds

### Command Categories

- **State Hydration** (5): `get_sam_logic`, `get_theme`, `get_mode`, `get_model_pins`, `is_founder`
- **Mode Control** (2): `set_mode`, `set_model_pin`
- **Dispatch** (4): `route_intent_live`, `dispatch_smart`, `dispatch_expert`, `execute_orchestration_loop`
- **Security** (6): `get_security_log`, `get_pending_report`, `approve_report`, `dismiss_report`, `validate_egress`, `add_egress_domain`
- **Sandbox** (5): `sandbox_execute`, `sandbox_apply`, `sandbox_snapback`, `sandbox_history`, `sync_to_host`
- **Message Queue** (2): `resync_messages`, `queue_message`
- **Bridge** (4): `bridge_handshake`, `bridge_pulse`, `bridge_pending`, `bridge_local_missions`, `bridge_execute_mission`
- **Sidecar** (6): `sidecar_status`, `sidecar_provision`, `sidecar_boot`, `sidecar_shutdown`, `sidecar_health`, `sidecar_health_check`
- **VSOck** (2): `vsock_ping`, `vsock_send_mission`
- **Shadow** (5): `shadow_scan_workspace`, `shadow_get_hotspots`, `shadow_get_atlas`, `shadow_status`, `shadow_generate`
- **Memory** (6): `memory_store`, `memory_query`, `memory_list`, `memory_delete`, `memory_provision_status`, `memory_provision_start`

## Tech Stack

- **Frontend**: React + TypeScript + Vite
- **Backend**: Rust (Tauri 2.0)
- **Type Safety**: specta + tauri-specta → auto-generated `src/bindings.ts`
- **Manifest**: `SAM_LOGIC.yaml` (baked in at compile time)

## Key Files

| File | Purpose |
|------|---------|
| `src-tauri/src/lib.rs` | All 48 IPC commands + app state |
| `src-tauri/src/orchestrator/mod.rs` | Core types (SamLogic, PipelineResult, etc.) |
| `src-tauri/src/orchestrator/router.rs` | Triage, dispatch, orchestration loop |
| `src-tauri/src/orchestrator/sandbox.rs` | Process isolation + sandboxed execution |
| `src-tauri/src/orchestrator/sentinel.rs` | PII-scrubbed crash reporting |
| `src-tauri/src/orchestrator/egress.rs` | Network allowlist filter |
| `src-tauri/src/orchestrator/shadow_scanner.rs` | Workspace file tree + tech stack detection |
| `src-tauri/src/orchestrator/shadow_temporal.rs` | Git 30-day churn → hot spots |
| `src-tauri/src/orchestrator/shadow_gen.rs` | LLM-powered shadow doc generation |
| `src-tauri/SAM_LOGIC.yaml` | Intelligence manifest |
| `src/App.tsx` | Main React component |
| `src/SidecarMonitor.tsx` | VM sidecar telemetry panel |
| `src/MemoryBrowser.tsx` | Memory Browser: search, filter, view/delete brain cells |
| `src/bindings.ts` | **Auto-generated** typed IPC wrappers |

## Memory Module (`src-tauri/src/memory/`)

| File | Purpose |
|------|---------|
| `store.rs` | `MemoryManager` — dual-tier LanceDB (Global + Project), schema, dedup, TTL prune |
| `embedder.rs` | `Embedder` — Hardware Handshake (CoreML/DirectML/CUDA→CPU) + ONNX + Gemini cloud fallback |
| `provisioner.rs` | `ModelProvisioner` — BGE-M3 INT8 ONNX download from HuggingFace |
| `buffer.rs` | `DreamBuffer` — SQLite buffer for memories captured while model downloads |
| `router.rs` | Reflective Hook — auto-extracts memories from LLM responses via Gemini |
| `mod.rs` | Module declaration |
