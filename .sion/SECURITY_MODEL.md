# S-ION Security Model

## 4 Defense Layers

S-ION employs a **defense-in-depth** strategy with 4 complementary security layers:

### Layer 1: Egress Filter (`egress.rs`)
- **What**: Domain-level allowlist for ALL outbound network requests
- **How**: Every API call goes through `validate_egress(url, agent_key)` → pass/block
- **Config**: Allowlist defined in `SAM_LOGIC.yaml → privacy.egress_allowlist`
- **Runtime**: Users can add trusted domains via `add_egress_domain()`
- **Logging**: Every request logged as `SecurityEvent` {timestamp, domain, status, agent_key}
- **IPC**: `get_security_log` → `Vec<SecurityEvent>`, `validate_egress`, `add_egress_domain`

### Layer 2: Sentinel (`sentinel.rs`)
- **What**: PII-scrubbed crash/error reporting with **explicit user consent**
- **How**: Errors produce a `SentinelReport` → shown in consent toast → user approves/dismisses
- **Privacy**: All reports are stripped of personal information before display
- **Founder Mode**: `is_founder()` check enables hidden telemetry tab (Ctrl+Shift+S)
- **IPC**: `get_pending_report` → `Option<SentinelReport>`, `approve_report`, `dismiss_report`

### Layer 3: Deterministic Audit Hook (`router.rs`)
- **What**: Zero-cost constitution enforcement before any code execution
- **How**: Pattern matching against `audit_rules.reject_if` patterns
- **Cost**: 0ms, $0.00 — no LLM call needed
- **Enforcement**: Blocks file deletion, network access, system modifications

### Layer 4: Sandbox (`sandbox.rs`)
- **What**: Process-level isolation for ALL code execution
- **How**:
  - **macOS**: `sandbox-exec` with custom profile (no network, no file writes outside temp)
  - **Linux/Windows**: VM sidecar (microVM or WSL2)
- **Features**:
  - Snap-Back: Restore pre-execution state with `sandbox_snapback()`
  - Execution History: `sandbox_history()` returns all past executions with diffs
  - Host Sync: `sync_to_host()` copies approved changes (path-confined)
- **IPC**: `sandbox_execute`, `sandbox_apply`, `sandbox_snapback`, `sandbox_history`, `sync_to_host`

## Security Rules

> **NEVER bypass the egress filter.** All external requests MUST go through `validate_egress()`.
>
> **NEVER write files outside the sandbox.** Use `sync_to_host()` for approved changes.
>
> **NEVER auto-send crash reports.** Always show the consent toast first.
>
> **NEVER remove audit hook patterns.** They are constitutional law.
