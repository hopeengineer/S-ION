# S-ION — Built-in AI Browser

### *Sovereign Intelligence. Universal Accessibility.*

**S-ION** is a standalone, AI-native workstation designed to bridge the gap between high-level engineering autonomy and "Grandma-proof" simplicity. Built with a **Local-First** philosophy, S-ION treats the world's most powerful LLMs (Claude, Gemini, DeepSeek, Kimi, GPT) as sub-processors while keeping your data, memory, and execution environment strictly isolated.

---

## 🏗️ Architecture: The "Fortress" Model

S-ION operates on a **Defense-in-Depth** chain, ensuring that AI-generated code never touches your host system without explicit consent.

```
Intent → Scrubber (PII) → Triage → Agent → Audit Hook → Sandbox → Egress Filter → Execute
```

| Layer | Component | Technology |
|---|---|---|
| **Core Engine** | Desktop shell + IPC commands | Tauri 2.0 (Rust) + React (TypeScript) + Vite |
| **Orchestrator** | Multi-model triage and routing | Gemini Flash → 8-agent swarm (SAM_LOGIC.yaml) |
| **Sandbox** | Code isolation + Snap-Back undo | macOS `sandbox-exec` / Firecracker MicroVMs (Linux) — *Phase 5/6* |
| **Bridge** | Persistent remote missions | Railway-hosted Node.js/TS service + SQLite |
| **Sentinel** | Privacy-first telemetry | Triple-Pass PII Scrubber + user consent flow |
| **Egress Filter** | Network allowlist gate | Domain-level blocking for all outbound AI calls |

---

## 🚀 Key Features

### Smart Mode — "Grandma" Layer

A minimalist, natural-language interface that translates complex system events into empathetic, human-readable actions.

- **Action Cards** — Preview AI-generated diffs before execution
- **Grandma-Speak Errors** — Cryptic API errors become plain English
- **Blood Orange Design** — High-contrast, accessible design language

### Expert Mode — "Sam" Layer

The engineering cockpit for power users.

- **Model Switchboard** — Pin specific models (DeepSeek, Kimi, Claude, GPT) to task categories
- **Security Dashboard** — Real-time Egress Filter log + sandbox health
- **Sentinel Console** — Scrubbed, anonymous telemetry with opt-in consent

### Snap-Back Safety Net

Every agent mission runs in a hardware-isolated sandbox. If an AI agent breaks something, one-click **Snap-Back** restores the pre-execution state instantly.

---

## 📂 Project Structure

```
S-ION/
├── docs/                       # Architecture & design decision logs
│   └── ROUTER_MANIFEST.md      # Multi-model routing specification
├── src/                        # React frontend (Vite)
│   ├── App.tsx                 # Main application UI
│   └── App.css                 # Blood Orange design system
├── src-tauri/                  # Rust backend (The Brain)
│   ├── SAM_LOGIC.yaml          # Constitutional rules + swarm config
│   ├── .env.example            # Template for required API keys
│   └── src/
│       ├── lib.rs              # IPC commands + AppState
│       └── orchestrator/       # The multi-model engine
│           ├── mod.rs          # Config structs + pipeline
│           ├── router.rs       # Triage + Smart/Expert dispatch
│           ├── sandbox.rs      # Code isolation + Snap-Back
│           ├── sentinel.rs     # PII Scrubber + telemetry
│           ├── egress.rs       # Domain allowlist gate
│           ├── heartbeat.rs    # Bridge connection + mission queue
│           └── translator.rs   # Grandma-Speak error translator
├── sentinel-bridge/            # Railway-hosted unified bridge
│   ├── src/                    # TypeScript Express API
│   ├── Dockerfile              # Railway deployment
│   └── README.md               # Bridge-specific documentation
└── public/                     # Static assets (S-ION icon)
```

---

## 🛠️ Getting Started

### Prerequisites

| Tool | Version | Purpose |
|---|---|---|
| **Rust** | Latest stable via `rustup` | Tauri backend |
| **Node.js** | v25+ | Frontend build + Bridge |
| **macOS / Linux / Windows** | Any | macOS uses `sandbox-exec`, Linux uses Firecracker |

### Setup

```bash
# 1. Clone the repository
git clone https://github.com/hopeengineer/S-ION.git && cd S-ION

# 2. Copy the env template and add your API keys
cp src-tauri/.env.example src-tauri/.env

# 3. Install frontend dependencies
npm install

# 4. Run in development mode
npm run tauri dev
```

### Required API Keys

| Key | Provider | Agent(s) |
|---|---|---|
| `KIMI_API_KEY` | Moonshot AI | Commander (Kimi K2.5) |
| `ANTHROPIC_API_KEY` | Anthropic | Audit Hook (Opus 4.6), Builder (Sonnet 4.6) |
| `GEMINI_API_KEY` | Google | Visionary (Gemini 3.1 Pro), Triage (Flash), Designers |
| `OPENAI_API_KEY` | OpenAI | Scout (GPT-5.3) |
| `DEEPSEEK_API_KEY` | DeepSeek | Analyst (V3.2), Grandma Translator |
| `SION_BRIDGE_TOKEN` | Self-hosted | Railway Bridge authentication |

---

## 🧬 Engineering Principles

Defined in [`SAM_LOGIC.yaml`](src-tauri/SAM_LOGIC.yaml):

- **Zero-Trust AI** — All AI output is sandboxed and audited before execution
- **Privacy First** — Triple-Pass PII Scrubber strips data before any telemetry leaves the device
- **Zero Assumptions** — Agents must never hallucinate; if unsure, they search or admit ignorance
- **Economic Routing** — Use the cheapest model that won't fail the task
- **Semantic Integrity** — No "div-soup." High-quality, functional code only

---

## 🗺️ Roadmap

- [x] Phase 1 — Browser shell + SAM_LOGIC constitution
- [x] Phase 2 — Multi-model orchestrator (8 agents, Smart/Expert modes)
- [x] Phase 3 — Grandma-Speak error translation
- [x] Phase 4 — Sentinel telemetry + Egress Filter + Railway Bridge
- [x] Phase 5 — Sandbox (process isolation + Snap-Back)
- [ ] Phase 6 — Vsock Handshake + Sidecar Manager (Firecracker integration)
- [ ] Phase 7 — Guest Agent (in-VM command executor)
- [ ] Phase 8 — CoPaw multi-channel missions (WhatsApp/iMessage/Discord)

---

## 📄 License

Proprietary. © 2026 S-ION PVT LTD. All rights reserved.
