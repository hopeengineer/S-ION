---
description: S-ION Development Master Instructions — auto-applies to every S-ION prompt
---

# S-ION Development Workflow

## Before Starting Any Work

// turbo-all

1. Read `.gemini/GEMINI.md` — context loading rules and architecture invariants
2. Read the relevant `.sion/` docs for the area you're working on
3. Read `src/bindings.ts` to understand the current IPC contract

## During Development

4. Run `cargo check` after every Rust change
5. If you modified any specta-annotated struct or command, run `cargo run` to regenerate `bindings.ts`
6. Run `npx tsc --noEmit` after any TypeScript changes

## After Every Structural Change

> **MANDATORY**: If your change touches any of the following, you MUST update the corresponding `.sion/` doc:

| What Changed | Update This |
|-------------|-------------|
| Added/removed IPC command | `.sion/ARCHITECTURE.md` (command catalog) |
| Added/removed/changed agent | `.sion/ARCHITECTURE.md` (agent roster) |
| Modified security logic, BLOCKED_PATTERNS, egress | `.sion/SECURITY_MODEL.md` |
| Added React state or Rust AppState field | `.sion/STATE_MACHINE.md` |
| Changed API provider, auth pattern, or error handling | `.sion/RUNTIME.md` |
| Made a significant design decision | `.sion/DECISIONS.md` |
| Hit a non-obvious bug or gotcha | `.sion/GOTCHAS.md` |

7. Update the relevant `.sion/` doc(s) per the table above
8. Verify `bindings.ts` is current (restart dev server if needed)

## Verification Checklist

9. `cargo check` — zero errors
10. `cargo run` — app starts, "📝 TypeScript bindings exported" appears
11. `npx tsc --noEmit` — zero errors (bindings.ts warnings are expected)
12. Frontend renders correctly in the Tauri window
