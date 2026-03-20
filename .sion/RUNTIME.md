# S-ION Runtime Flow

## Actual API Call Chain

### Smart Mode: `execute_orchestration_loop`
```
Frontend: commands.executeOrchestrationLoop(intent, workspaceRoot)
    │
    ▼  (1) Gemini Flash Triage  ────────────────────────────
    │   POST https://generativelanguage.googleapis.com/v1beta
    │        /models/gemini-3.1-flash-lite-preview:generateContent
    │   Body: system_instruction + user intent
    │   Config: temperature=0.05, maxOutputTokens=512, responseMimeType=application/json
    │   Response: {"category":"simple_qa","route_to":"analyst","reasoning":"...","confidence":0.95}
    │
    │   ⚠️  RECOVERY: If JSON is truncated, extract_field() recovers category+route_to
    │      from partial JSON. Recovered results get confidence=0.7.
    │
    ▼  (1b) Memory Recall ───────────────────────────
    │   embed(intent) via CoreML ONNX → search LanceDB top 3 → append as [Memory Recall]
    │   Graceful: if no memories or embedder not ready, proceeds without recall
    │
    ├── Knowledge Track (simple_qa, long_context, image_gen)
    │   │
    │   ▼  (2) Call routed agent ──────────────────────────
    │   │   analyst → POST https://api.deepseek.com/v1/chat/completions (DeepSeek)
    │   │   visionary → POST https://generativelanguage.googleapis.com/v1beta (Gemini Pro)
    │   │   fast_designer → POST https://generativelanguage.googleapis.com/v1beta (NanoBanana)
    │   │
    │   ▼  (2b) Reflective Hook ──────────────────────────
    │   │   POST to Gemini Flash: extract memories from response
    │   │   Stores to LanceDB via embed() + MemoryManager.store()
    │   │
    │   └── Result: OrchestrationResult { track: "knowledge", response: "...", ... }
    │
    └── Action Track (deep_code, parallel_ui)
        │
        ▼  (2) Commander generates ActionEnvelope ────────
        │   POST https://api.moonshot.ai/v1/chat/completions (Kimi K2.5)
        │   System prompt: ACTION_SYSTEM_PROMPT (forces JSON schema)
        │   Config: temperature=1.0, max_tokens=4096, response_format=json_object
        │   Response: {"mission_id":"...","explanation":"...","bash_commands":["..."],"target_files":["..."]}
        │
        │   ⚠️  Always uses Commander regardless of triage route_to
        │
        ▼  (3) Deterministic Audit ───────────────────────
        │   audit_envelope() → checks bash_commands against BLOCKED_PATTERNS
        │   0ms, $0.00 — pure string matching, no LLM call
        │   Blocks: curl, wget, ssh, sudo, rm -rf /, fork bombs, path traversal
        │
        ▼  (4) Sandbox Execution ─────────────────────────
        │   sandbox.execute(combined_script, agent_key)
        │   macOS: sandbox-exec with custom profile (no network, no host writes)
        │   Creates: snapshot (for Snap-Back), temp dir, executes script
        │   Returns: SandboxResult { stdout, stderr, exit_code, file_changes, duration_ms }
        │
        └── Result: OrchestrationResult { track: "action", sandbox_result: ..., ... }
```

### Startup Auto-Provision Flow
```
App starts → setup hook → tokio::spawn
  ├── Model already downloaded? → init_local() + MemoryManager::init()
  └── First launch? → hippocampus_provision():
      1. provisioner.provision() — downloads BGE-M3 INT8 from gpahal/bge-m3-onnx-int8
      2. embedder.init_local() — loads ONNX via CoreML/DirectML/CUDA/CPU
      3. MemoryManager::init() — opens LanceDB at ~/Library/Application Support/com.s-ion.dev/

Dreaming Loop (every 5 min):
  1. Flush unpromoted DreamBuffer entries → embed + store in LanceDB
  2. prune_expired() — remove observations > 30 days
  3. cleanup_promoted() — delete buffer entries > 7 days
```

### Expert Mode: `dispatch_expert`
```
Frontend: commands.dispatchExpert(intent, taskCategory)
    │
    ├── Resolve pinned agent from ExpertPins (or default to analyst)
    ├── Call that single agent via call_agent_for_response()
    └── Return DispatchResult { mode: "expert", response: "...", ... }
```

## API Provider Matrix

| Agent | Provider | API | Auth Pattern | Response Path |
|-------|----------|-----|-------------|---------------|
| Triage | Gemini | REST (not OpenAI) | `?key=` query param | `candidates[0].content.parts[0].text` |
| Analyst | DeepSeek | OpenAI-compatible | `Bearer` header | `choices[0].message.content` |
| Commander | Kimi/Moonshot | OpenAI-compatible | `Bearer` header | `choices[0].message.content` |
| Builder | Kimi/Moonshot | OpenAI-compatible | `Bearer` header | `choices[0].message.content` |
| Visionary | Gemini | REST | `?key=` query param | `candidates[0].content.parts[0].text` |
| Scout | OpenAI | OpenAI-compatible | `Bearer` header | `choices[0].message.content` |
| Fast Designer | Gemini | REST | `?key=` query param | `candidates[0].content.parts[0].text` |
| Pro Designer | Gemini | REST | `?key=` query param | `candidates[0].content.parts[0].text` |

> **CRITICAL**: Gemini uses `?key=` auth + custom JSON body. Everyone else uses `Bearer` + OpenAI-compatible body.

## Error Propagation

```
API call fails → Err(String) → serialized to JSON → Result<String,String> on IPC
    → Frontend receives {status: "error", error: "..."} from commands.*()
    → Frontend throws new Error(result.error) → caught by try/catch → shown in UI

Triage JSON truncated → extract_field() recovery → confidence=0.7
Triage completely fails → Err → whole operation fails → error shown in UI
Audit fails → Err("BLOCKED: ...") → sandbox never runs → error shown in UI
Sandbox times out → SandboxResult { timed_out: true } → Action Card shows timeout
```

## Bridge (Railway) Flow
```
handshake() → GET /health → GET /bridge/pending (auth check)
pulse()     → GET /bridge/dequeue (claims next mission)
pending()   → GET /bridge/pending (count only)
Auth: Bearer SION_BRIDGE_TOKEN header
```

## Environment Variables Required
```
GEMINI_API_KEY     — Triage, Visionary, Fast Designer, Pro Designer
KIMI_API_KEY       — Commander, Audit Hook, Builder
DEEPSEEK_API_KEY   — Analyst
OPENAI_API_KEY     — Scout
SION_BRIDGE_TOKEN  — Railway bridge auth (optional)
```
