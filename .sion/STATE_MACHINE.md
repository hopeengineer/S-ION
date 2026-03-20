# S-ION State Machine

## React State (Frontend)

| State Variable | Type | Source IPC Command |
|---|---|---|
| `mode` | `"smart" \| "expert"` | `get_mode()` on mount |
| `theme` | `"light" \| "dark"` | localStorage + `get_theme()` for accent |
| `pins` | `Record<string, string>` | `get_model_pins()` on mount |
| `isFounder` | `boolean` | `is_founder()` on mount |
| `securityLog` | `SecurityEvent[]` | `get_security_log()` polled every 2s |
| `pendingReport` | `SentinelReport \| null` | `get_pending_report()` polled every 2s |
| `sandboxHistory` | `SandboxResult[]` | `sandbox_history()` on demand |
| `sidebarTab` | `"cockpit" \| "security" \| "sentinel" \| "shadow" \| "memory"` | Local state |
| `smartResult` | `DispatchResult \| null` | `dispatch_smart()` / `execute_orchestration_loop()` |
| `orchResult` | `OrchestrationResult \| null` | `execute_orchestration_loop()` |
| `actionCard` | `SandboxResult \| null` | From orchestration result |

## Rust State (`AppState`)

| Field | Type | Mutex? | Purpose |
|---|---|---|---|
| `sam_logic` | `SamLogic` | No (immutable) | Intelligence manifest |
| `mode` | `RuntimeMode` | Yes | Smart/Expert toggle |
| `expert_pins` | `ExpertPins` | Yes | Model→task pinning |
| `pending_resync` | `Vec<String>` | Yes | Message queue for sleep/wake |
| `egress` | `EgressFilter` | Yes | Network allowlist |
| `sentinel` | `Sentinel` | No (internal Mutex) | Crash reporting |
| `sandbox` | `Sandbox` | Yes | Execution isolation |
| `heartbeat` | `BridgeHeartbeat` | No (internal Mutex) | Railway bridge connection |
| `sidecar` | `SidecarManager` | Yes | VM lifecycle |
| `vsock` | `VsockChannel` | Yes | Guest Agent communication |
| `memory` | `Option<MemoryManager>` | Yes (tokio) | LanceDB vector store |
| `embedder` | `Embedder` | Yes (tokio) | ONNX/CoreML text embedder |
| `provisioner` | `Option<ModelProvisioner>` | Yes (tokio) | BGE-M3 model download manager |
| `dream_buffer` | `Option<DreamBuffer>` | Yes (std) | SQLite buffer for pre-model memories |

## Data Flow: Intent → Response

```
Frontend                          Rust Backend
─────────────────────────────────────────────────────
handleSubmit()
  │
  ├─ Smart Mode:
  │   invoke("execute_orchestration_loop")
  │       │
  │       ├── call_gemini_flash_triage()  ← validate_egress()
  │       ├── Memory Recall: embed(intent) → LanceDB search top 3 → inject as context
  │       ├── Knowledge? → call_agent_for_response() → Reflective Hook extracts memories
  │       └── Action?    → parse envelope → sandbox.execute()
  │                                             │
  │                          ┌──────────────────┘
  │                          ▼
  │   setOrchResult(result)  ← OrchestrationResult
  │
  └─ Expert Mode:
      invoke("dispatch_expert")
        │
        └── call single pinned agent ← validate_egress()
```

## SidecarMonitor State

| State | IPC Command | Poll Interval |
|---|---|---|
| `status` | `sidecar_status()` | 2s |
| `health` | `sidecar_health()` | 3s (only when running) |
| Actions: `sidecar_provision()`, `sidecar_boot()`, `sidecar_shutdown()` | On user click |
