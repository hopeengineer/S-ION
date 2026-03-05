# S-ION Multi-Model Router Manifest

> The "Brain" of the browser — defines how S-ION routes every user intent to the optimal AI model.

## Global Provider Registry

| Provider | Primary Model | Strength | Role in S-ION |
|----------|--------------|----------|---------------|
| **Claude** | Opus 4.6 / Sonnet 4.6 | Deep Logic / Coding | Guardian & Architect |
| **Kimi** | K2.5 | Agent Swarms / UI | Commander & Visualizer |
| **DeepSeek** | V3 / R1 | Reasoning / Cheap Logic | Analyst & Generalist |
| **Gemini** | 3.1 Pro / Flash | Multi-modal / Context | Scout & Researcher |
| **OpenAI** | GPT-5 / o3 | Tool Calling / Speed | Operator & Executor |

## Smart Mode: Auto-Dispatcher

Gemini Flash triages every intent into one of 4 categories:

| Triage | Question | Route To | Why |
|--------|----------|----------|-----|
| **A** | Simple question or summary? | DeepSeek V3 | Cheapest high-quality logic |
| **B** | UI build or parallel research? | Kimi K2.5 | Swarm mode for speed |
| **C** | Complex code refactor or security audit? | Sonnet 4.6 / Opus 4.6 | Deep reasoning required |
| **D** | Long video or 500-page PDF? | Gemini 3.1 Pro | Massive context window |

## Expert Mode: Manual Cockpit

| Task Category | Default (Sam's Choice) | Manual Options |
|---------------|----------------------|----------------|
| Terminal / CLI | GPT-5 mini | DeepSeek, Claude, Kimi |
| Frontend Coding | Kimi K2.5 | Sonnet, GPT-5, DeepSeek |
| Logic Auditing | Opus 4.6 | DeepSeek R1, Sonnet |
| Web Research | DeepSeek V3 | Gemini, Kimi, GPT-5 |
| Grandma Mode UI | Gemini Flash | DeepSeek, GPT-5 |

## Pipeline Flow

```
Smart Mode:  Intent → Gemini Flash (triage) → Best Model → Opus Audit Hook → Execute
Expert Mode: Intent → User's Pinned Model → Opus Audit Hook → Execute
```
