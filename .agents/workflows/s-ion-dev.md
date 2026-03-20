---
description: S-ION Development Master Instructions — auto-applies to every S-ION prompt
---

# S-ION Development Master Instructions

// turbo-all

## 📋 Before Starting Any Work

1. Read `.gemini/GEMINI.md` — context loading rules and architecture invariants
2. Read the relevant `.sion/` docs for the area you're working on
3. Read `src/bindings.ts` to understand the current IPC contract

## 🔧 During Development

4. Run `cargo check` after every Rust change
5. If you modified any specta-annotated struct or command, run `cargo run` to regenerate `bindings.ts`
6. Run `npx tsc --noEmit` after any TypeScript changes

## 📝 After Every Structural Change

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

## ✅ Verification Checklist

9. `cargo check` — zero errors
10. `cargo run` — app starts, "📝 TypeScript bindings exported" appears
11. `npx tsc --noEmit` — zero errors (bindings.ts warnings are expected)
12. Frontend renders correctly in the Tauri window

---

## 🧠 Model Switching Protocol

When working on S-ION, **proactively recommend model switches** based on the task type:

### Use Kimi K2.5 ("The Commander") for:
- Primary intent decomposition and parallel execution planning
- Multi-step task orchestration across sub-agents
- Tool-call planning and dependency resolution
- High-context decision-making (128K native context)

> **Trigger phrase:** "🎯 This task involves multi-agent orchestration — recommend switching to **Kimi K2.5**."

### Use Claude Opus 4.6 ("The Audit Hook") for:
- **Auditing** Kimi's execution plans before Firecracker MicroVMs boot
- System architecture changes and design decisions
- Rust backend code (Tauri IPC bridge, MCP host, orchestrator)
- Security-sensitive logic (privacy interceptors, encryption, sandboxing)
- `SAM_LOGIC.yaml` enforcement and reasoning
- Natural Language → Shell command translation (safety-critical)

> **Trigger phrase:** "⚙️ This task requires deep reasoning / security audit — recommend switching to **Opus 4.6**."

### Use Gemini 3.1 Pro ("The Visionary") for:
- UI/UX visual audits and design polish
- Multimodal tasks (screenshot analysis, video summarization)
- Web research, scraping, and content summarization
- Large-context document analysis (2M+ token window)
- Quick visual verification of Blood Orange theming

> **Trigger phrase:** "👁️ This task is visual/multimodal — recommend switching to **Gemini 3.1 Pro**."

### Use Claude Sonnet 4.6 ("The Builder") for:
- Parallel code generation and refactoring
- Building React components and frontend features
- Routine implementation tasks with clear specs
- Test writing and documentation

### Use GPT-5.3 Instant ("The Scout") for:
- Low-latency web browsing and form-filling
- Quick cross-app communication tasks
- Simple, fast lookups

## 🎨 S-ION Design Language

| Token | Value |
|---|---|
| Background | `#FFFFFF` |
| Accent | `#FF4500` (Blood Orange) |
| Typography | Inter (UI) / System Monospace (code) |
| CSS | Tailwind-first. **Strictly NO inline styles.** |
| HTML | Semantic HTML5 only. No div-soup. |
| Logic | Type-safe TypeScript. Functional patterns over OOP. |

## 🏗️ Architecture Principles

1. **Rust is the Brain** — All API keys, orchestration, and LLM routing run in Rust. The React frontend is a "blind painter."
2. **Two-Stage Pipeline** — Kimi K2.5 (Commander) plans → Opus 4.6 (Audit Hook) verifies → then dispatch.
3. **LanceDB for Memory** — Local AES-256 encrypted vector store. Zero cloud sync.
4. **MCP Dual Mode** — Client (pull local tools) + Server (expose to CoPaw mobile bridge).
5. **Firecracker Sandbox** — All third-party code runs in ephemeral MicroVMs. Opus must approve before boot.
6. **Frameless Custom Chrome** — Own every pixel with Blood Orange design language.
7. **CoPaw Cloud Bridge** — Railway-hosted Node.js gateway for WhatsApp/iMessage/Discord.
8. **Re-sync on Wake** — Rust backend handles async message queue re-sync when machine wakes from sleep.

## 🔒 Privacy Fortress

- Intercept and scrub all outgoing headers in Rust
- Block OS telemetry from Native WebView
- Local-only storage, zero cloud sync
- All execution in Firecracker Sandbox

## 🐛 Debugging Protocol

- **NEVER speculate** on the cause of a bug. Always write a script or test to reproduce, diagnose, and verify the fix.
- Do not say "likely" or "probably" when diagnosing issues. Run actual code to confirm.
- Keep debugging until the issue is solved with proof (passing test, successful curl, etc.).

## 📁 Project Structure

```
S-ION/
├── src/                    # React frontend (Vite)
├── src-tauri/              # Rust backend (Tauri 2.0)
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── orchestrator/   # Smart Router, LLM dispatch
│   │   ├── mcp/            # MCP Client + Server
│   │   ├── memory/         # LanceDB ReMe integration
│   │   └── sandbox/        # Firecracker MicroVM manager
│   └── SAM_LOGIC.yaml      # Logic Manifest
├── copaw-bridge/           # Railway cloud bridge (Node.js)
└── .github/workflows/      # CI/CD
```
