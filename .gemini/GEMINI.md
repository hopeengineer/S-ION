# S-ION – AI Context Loading Rules

## Before Making Any Changes

1. **Read `.sion/ARCHITECTURE.md`** — Understand the swarm paradigm and two-track pipeline
2. **Read `.sion/SECURITY_MODEL.md`** — The 4 defense layers are constitutional law. **DO NOT bypass the egress filter or remove audit hook patterns.**
3. **Read `.sion/STATE_MACHINE.md`** — Understand React↔Rust state mapping before touching any state
4. **Read `src/bindings.ts`** — These are the exact IPC contracts. Every frontend `invoke()` call must match a typed wrapper here.

## IPC Contract Rules

- All 37 IPC commands in `src-tauri/src/lib.rs` are annotated with `#[specta::specta]`
- `src/bindings.ts` is **auto-generated** by `tauri-specta` on every debug build — **DO NOT manually edit it**
- When adding a new command: annotate with both `#[tauri::command]` and `#[specta::specta]`, add to the `collect_commands!` list in `run()`
- All custom types must derive `specta::Type` alongside `Serialize`/`Deserialize`

## Architecture Invariants

- **SAM_LOGIC.yaml** is baked in at compile time via `include_str!` — changes require rebuild
- **Sandbox isolation** is mandatory for all code execution — never run user code on the host directly
- **Egress validation** happens before every external API call
- **Consent toast** must be shown before sending any crash report data

## File Organization

```
src-tauri/
  src/
    lib.rs           ← 37 IPC commands + AppState
    main.rs          ← Entry point (calls lib::run())
    orchestrator/
      mod.rs         ← Core types (SamLogic, PipelineResult, etc.)
      router.rs      ← Triage, dispatch, orchestration
      sandbox.rs     ← Process isolation
      sentinel.rs    ← Crash reporting
      egress.rs      ← Network allowlist
      heartbeat.rs   ← Railway bridge
      sidecar_manager.rs ← VM lifecycle
      vsock_proto.rs ← Guest Agent protocol
  SAM_LOGIC.yaml     ← Intelligence manifest

src/
  App.tsx            ← Main React component
  SidecarMonitor.tsx ← VM telemetry panel
  bindings.ts        ← AUTO-GENERATED (do not edit)
```
