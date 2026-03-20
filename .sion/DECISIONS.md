# S-ION Design Decisions

This documents the **why** behind every major architectural choice. Read this before questioning or changing anything.

## Why 8 Agents?

Each agent maps to a fundamentally different capability:
- **Commander**: Needs structured output (ActionEnvelope JSON) — requires a model that reliably follows schemas
- **Audit Hook**: Constitutional enforcement — same model as Commander so it "thinks like" the Commander
- **Analyst**: High-volume Q&A at minimal cost — DeepSeek is 50-100x cheaper than GPT-4
- **Visionary**: Multimodal (images, video, PDFs) — only Gemini Pro has native multimodal
- **Builder**: Complex code generation — needs large context window + code specialization
- **Scout**: Real-time tool calling (browsing, forms) — GPT-5 Mini has fastest latency
- **Fast Designer**: Rapid image prototyping — NanoBanana is instant
- **Pro Designer**: High-fidelity assets — NanoBanana Pro is production-quality

## Why Gemini Flash for Triage (Not a Local Model)?

- **Speed**: ~200ms classification — faster than loading a local model
- **Cost**: Flash Lite is near-free ($0.00 per classification)
- **Accuracy**: Pre-trained on intent patterns — no fine-tuning needed
- **Temperature: 0.05**: Near-deterministic — same input → same route

## Why Commander Always Handles Action Track?

Even if triage routes to "builder" or "analyst", the Action track always uses Commander (Kimi K2.5) because:
- Only Commander's system prompt is tuned for `ActionEnvelope` JSON output
- `response_format: json_object` is critical — not all models support it
- The agent triage selected is for *response quality*, not *structured output reliability*

## Why Deterministic Audit Hook (Not LLM-Based)?

The audit was originally designed as an LLM call. It was replaced with pattern matching because:
- **Speed**: 0ms vs ~2s for an LLM call
- **Cost**: $0.00 vs ~$0.003 per audit
- **Reliability**: LLMs can be tricked by prompt injection. Pattern matching cannot.
- **Completeness**: The `BLOCKED_PATTERNS` array covers every dangerous command class

## Why `serde_json::Value` Was Eliminated

Several commands originally returned `serde_json::Value` (untyped JSON). This was replaced with typed structs because:
- `specta` cannot generate TypeScript bindings for `serde_json::Value`
- Untyped JSON means the frontend has to guess the shape — silent bugs
- Typed structs create a compile-time contract between Rust and TypeScript

## Why u64 → u32 in Config Structs

TypeScript's `number` type uses IEEE 754 doubles (max safe integer: 2^53). `specta` forbids `u64` by default (`BigIntForbidden`) because:
- `u64` → `bigint` in TS, which doesn't JSON-serialize normally
- Config values (max_events=200, batch_interval=300s, duration_ms) never exceed u32 range
- Using `u32` keeps the generated TS types as `number` (no BigInt friction)

## Why sandbox-exec on macOS (Not Firecracker)?

`SAM_LOGIC.yaml` says "firecracker" but the actual implementation uses `sandbox-exec` because:
- Firecracker requires KVM (Linux only) — macOS doesn't support it
- `sandbox-exec` provides process-level sandboxing with custom profiles
- The YAML is the *aspirational* spec; the code handles platform detection

## Why Railway Bridge (Not Direct WebSocket)?

- **NAT traversal**: Desktop clients behind firewalls can't accept incoming connections
- **Queueing**: Missions queue when the client is offline, delivered on next pulse
- **Auth**: Token-based handshake ensures only authorized S-ION instances connect
- **Simplicity**: HTTP polling (pulse every few seconds) vs WebSocket complexity

## Why `include_str!` for SAM_LOGIC.yaml?

- **Compile-time baking**: The manifest is embedded in the binary — no file system dependency
- **Immutability**: Changes require a rebuild — prevents runtime tampering
- **Single source of truth**: One YAML file defines all agent configs, security rules, and audit patterns

## Why CoreML-First for Embeddings (Not Cloud-Only)?

- **Privacy**: Embedding vectors never leave the machine — zero cloud sync for memory content
- **Speed**: CoreML on Apple Neural Engine is ~10x faster than a round-trip to Gemini
- **Offline**: Works without internet after initial model download
- **Fallback**: Cloud Gemini embedding is the graceful fallback if ONNX fails or model isn't downloaded yet

## Why Auto-Provision on First Launch?

- **Zero friction**: Users shouldn't have to manually download models — that's a developer experience, not a user experience
- **Background**: Downloads happen in a spawned async task — the app is usable immediately via Gemini cloud fallback
- **Idempotent**: If files already exist, startup just initializes without re-downloading

## Why DreamBuffer (SQLite) Between ONNX and LanceDB?

- **Race condition**: Memory extraction can happen before the ONNX model finishes downloading
- **No data loss**: SQLite captures everything immediately, promoted to LanceDB when the model is ready
- **Transactional**: Each entry is marked `promoted=1` only AFTER the LanceDB write succeeds

## Why Reflective Hook (Not Explicit Memory Commands)?

- **Invisible**: Users don't have to manually "save" information — S-ION extracts knowledge automatically
- **Comprehensive**: The LLM identifies facts, preferences, and decisions — humans miss what they know
- **Deduplicated**: MemoryManager checks for semantic similarity before storing — no duplicates
