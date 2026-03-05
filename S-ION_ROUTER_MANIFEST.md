# S-ION Multi-Model Router Manifest

> The "Brain" of the browser — defines how S-ION routes every user intent to the optimal AI model.

## Global Provider Registry

| Provider | Primary Model | Strength | Role in S-ION |
|----------|--------------|----------|---------------|
| **Claude** | Opus 4.6 / Sonnet 4.6 | Deep Logic / Coding | Guardian & Builder |
| **Kimi** | K2.5 | Agent Swarms / UI | Commander & Visualizer |
| **DeepSeek** | V3.2 / R1 | Reasoning / Cheap Logic | Analyst & Generalist |
| **Gemini** | 3.1 Pro / Flash | Multi-modal / Context | Visionary & Triage |
| **OpenAI** | GPT-5 / Mini | Tool Calling / Speed | Operator & Scout |
| **Gemini Image** | Nano Banana / Pro | Image Generation | Fast_Designer & Pro_Designer |

## Core Constitutional Rule

> **Zero Assumptions:** S-ION agents MUST NEVER assume facts, APIs, or states without verifying through research, search, or absolute proof. Hallucination is strictly forbidden; if an agent does not know definitively, it must admit it or execute a search tool.

## Smart Mode: Auto-Dispatcher

Gemini Flash triages every intent into one of 4 categories:

| Triage | Question | Route To | Why |
|--------|----------|----------|-----|
| **A** | Simple question or summary? | DeepSeek V3.2 | Cheapest high-quality logic |
| **B** | UI build or parallel research? | Kimi K2.5 | Swarm mode for speed |
| **C** | Complex code refactor or security audit? | Sonnet 4.6 / Opus 4.6 | Deep reasoning required |
| **D** | Long video or 500-page PDF? | Gemini 3.1 Pro | Massive context window |
| **E** | Image generation or visual concept? | Nano Banana | Fast native image generation |

## Expert Mode: Manual Cockpit

| Task Category | Default (Sam's Choice) | Manual Options |
|---------------|----------------------|----------------|
| Terminal / CLI | GPT-5 Mini | DeepSeek, Claude, Kimi |
| Frontend Coding | Kimi K2.5 | Sonnet 4.6, GPT-5, DeepSeek |
| Logic Auditing | Opus 4.6 | DeepSeek V3.2, Sonnet 4.6 |
| Web Research | DeepSeek V3.2 | Gemini, Kimi, GPT-5 |
| Image Generation | Nano Banana | Nano Banana Pro |
| Grandma Mode UI | Gemini Flash | DeepSeek, GPT-5 Mini |

## Pipeline Flow

```
Smart Mode:  Intent → Gemini Flash (triage) → Best Model → Opus Audit Hook → Execute
Expert Mode: Intent → User's Pinned Model → Opus Audit Hook → Execute
```
