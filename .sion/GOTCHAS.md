# S-ION Gotchas & Lessons Learned

Hard-won knowledge from building this codebase. Read this BEFORE making changes.

## 1. specta + u64 = Runtime Panic (NOT Compile Error)

**Problem**: `specta::Type` on a struct with `u64`/`usize` fields compiles fine but **panics at runtime** with `BigIntForbidden` when the export runs.

**Rule**: All specta-annotated structs MUST use `u32` or smaller for integer fields. If you need `u64` internally (e.g., `VecDeque::with_capacity`), cast with `as usize` at the call site.

**Why it's sneaky**: `cargo check` passes, `cargo build` passes, only `cargo run` panics because the export happens inside `run()` at app startup.

## 2. tauri-specta v2 ≠ v1 (Completely Different API)

**v1** (wrong): `specta::collect_types!` + `tauri_specta::ts::export()`
**v2** (correct): `tauri_specta::Builder::<tauri::Wry>::new().commands(collect_commands![...])` + `builder.export(Typescript::default(), path)` + `builder.invoke_handler()`

**Key difference**: v2 Builder REPLACES `tauri::generate_handler!` — commands are registered once in `collect_commands!`, not duplicated.

**Required crate**: `specta-typescript` (separate from `specta` and `tauri-specta`)

## 3. bindings.ts is Generated at RUNTIME, Not Build Time

The `builder.export()` call runs inside `run()` — meaning the app must actually **start** for `bindings.ts` to be written. `cargo build` alone produces an empty file.

**To regenerate**: Run `npm run tauri dev` or `cargo run` (debug builds only).

## 4. Commands Returning Result<T,E> Don't Throw in Frontend

Generated `commands.*()` for Rust functions returning `Result<String, String>` return `{status: "ok", data} | {status: "error", error}` — they do NOT throw exceptions.

**Pattern**:
```typescript
const res = await commands.executeOrchestrationLoop(intent, workspace);
if (res.status === "error") throw new Error(res.error);
const parsed = JSON.parse(res.data);
```

## 5. Gemini vs OpenAI: Different Auth + Response Shape

**Gemini**: `?key=API_KEY` in URL, response at `candidates[0].content.parts[0].text`
**OpenAI/Kimi/DeepSeek**: `Bearer` header, response at `choices[0].message.content`

Mixing these up = silent 401 or missing content. The `call_gemini_flash_triage` function handles Gemini's format; `call_openai_compatible` handles everyone else.

## 6. Truncated JSON Recovery in Triage

Gemini Flash sometimes truncates JSON responses. The triage has a recovery mechanism:
- `extract_field(content, "category")` + `extract_field(content, "route_to")`
- Recovers partial triage with `confidence: 0.7`
- If recovery fails → the whole operation errors

## 7. sandbox-exec, Not Firecracker

The YAML says "firecracker" but macOS uses `sandbox-exec`. Don't be confused by the aspirational config — always check the actual `Sandbox::new()` implementation for platform-specific behavior.

## 8. `file_changes` is Partial<> in Generated Types

specta generates `HashMap<String, T>` as `Partial<{ [key in string]: T }>`, meaning values are `T | undefined`. Always use optional chaining (`change?.status`) when accessing map values.

## 9. The Orchestration Loop Returns Serialized JSON

`execute_orchestration_loop` returns `Result<String, String>` — the `String` is a JSON-serialized `OrchestrationResult`. The frontend must `JSON.parse()` the data. This is by design: complex nested types go through string serialization.

## 10. Never Verify with Just `cargo check`

`cargo check` only verifies compilation. It does NOT:
- Run the specta export (runtime-only)
- Verify bindings are valid TypeScript
- Test that the app actually starts

**Full verification**: `cargo check` → `cargo run` (wait for "📝 TypeScript bindings exported") → `tsc --noEmit`
