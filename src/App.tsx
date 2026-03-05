import { useState, useCallback, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const appWindow = getCurrentWindow();

// ── Types ──

interface ExecutionStep {
  step_id: number;
  agent: string;
  action: string;
  tool_calls: string[];
  depends_on: number[];
}

interface ExecutionPlan {
  intent: string;
  steps: ExecutionStep[];
  reasoning: string;
}

interface AuditVerdict {
  approved: boolean;
  reasoning: string;
  violations: string[];
}

interface PipelineResult {
  stage: string;
  plan: ExecutionPlan | null;
  verdict: AuditVerdict | null;
  error: string | null;
}

interface TriageResult {
  category: string;
  route_to: string;
  reasoning: string;
  confidence: number;
}

interface DispatchResult {
  mode: string;
  triage: TriageResult | null;
  routed_to: string;
  model_name: string;
  designation: string;
  response: string | null;
  error: string | null;
}

// ── Model Registry (for Expert Mode UI) ──

const AGENTS = [
  { key: "commander", label: "Commander", model: "Kimi K2.5", emoji: "🎯", color: "#FF4500" },
  { key: "audit_hook", label: "Audit Hook", model: "Opus 4.6", emoji: "🛡️", color: "#6B6B6B" },
  { key: "analyst", label: "Analyst", model: "DeepSeek V3", emoji: "🔍", color: "#4A90D9" },
  { key: "visionary", label: "Visionary", model: "Gemini Pro", emoji: "👁️", color: "#34A853" },
  { key: "builder", label: "Builder", model: "Sonnet 4.6", emoji: "🔨", color: "#9B59B6" },
  { key: "scout", label: "Scout", model: "GPT-5.3", emoji: "🏃", color: "#F39C12" },
];

const TASK_CATEGORIES = [
  { key: "terminal_cli", label: "Terminal / CLI" },
  { key: "frontend_coding", label: "Frontend Coding" },
  { key: "logic_auditing", label: "Logic Auditing" },
  { key: "web_research", label: "Web Research" },
  { key: "grandma_mode_ui", label: "Grandma Mode" },
];

function App() {
  const [intent, setIntent] = useState("");
  const [mode, setMode] = useState<"smart" | "expert">("smart");
  const [loading, setLoading] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);

  // Smart Mode state
  const [smartResult, setSmartResult] = useState<DispatchResult | null>(null);
  const [pipeline, setPipeline] = useState<PipelineResult | null>(null);

  // Expert Mode state
  const [pins, setPins] = useState<Record<string, string>>({});
  const [selectedCategory, setSelectedCategory] = useState("terminal_cli");

  // Load pins from Rust backend on mount
  useEffect(() => {
    invoke<string>("get_model_pins").then((res) => {
      try { setPins(JSON.parse(res)); } catch { }
    });
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!intent.trim()) return;
    setLoading(true);
    setSmartResult(null);
    setPipeline(null);

    try {
      if (mode === "smart") {
        // Smart Mode: Gemini Flash triage → model dispatch
        const result = await invoke<string>("dispatch_smart", { intent });
        const parsed: DispatchResult = JSON.parse(result);
        setSmartResult(parsed);

        // If triage routes to commander, also run the full pipeline
        if (parsed.routed_to === "commander") {
          const pipeResult = await invoke<string>("route_intent_live", { intent });
          setPipeline(JSON.parse(pipeResult));
        }
      } else {
        // Expert Mode: use pinned model
        const result = await invoke<string>("dispatch_expert", {
          intent,
          taskCategory: selectedCategory,
        });
        const parsed: DispatchResult = JSON.parse(result);
        setSmartResult(parsed);
      }
    } catch (err) {
      console.error("Dispatch error:", err);
      // Fallback to heuristic
      try {
        const result = await invoke<string>("route_intent", { intent });
        setSmartResult({
          mode: "fallback",
          triage: null,
          routed_to: "heuristic",
          model_name: "Heuristic",
          designation: "Keyword Match",
          response: result,
          error: null,
        });
      } catch { }
    } finally {
      setLoading(false);
    }
  }, [intent, mode, selectedCategory]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleSubmit();
  };

  const toggleMode = async () => {
    const newMode = mode === "smart" ? "expert" : "smart";
    setMode(newMode);
    setSidebarOpen(newMode === "expert");
    await invoke("set_mode", { modeStr: newMode });
  };

  const handlePinChange = async (category: string, agentKey: string) => {
    setPins((prev) => ({ ...prev, [category]: agentKey }));
    await invoke("set_model_pin", { category, agentKey });
  };

  // Active agents for status bar
  const activeAgents = new Set<string>();
  if (smartResult) activeAgents.add(smartResult.routed_to);
  if (pipeline?.plan) {
    activeAgents.add("commander");
    if (pipeline.verdict) activeAgents.add("audit_hook");
    pipeline.plan.steps.forEach((s) => activeAgents.add(s.agent));
  }

  return (
    <main className={`app-layout ${sidebarOpen ? "sidebar-open" : ""}`}>
      {/* ── Custom Titlebar ── */}
      <header className="titlebar" data-tauri-drag-region>
        <section className="titlebar-traffic">
          <button className="traffic-light close" onClick={() => appWindow.close()} aria-label="Close" />
          <button className="traffic-light minimize" onClick={() => appWindow.minimize()} aria-label="Minimize" />
          <button className="traffic-light maximize" onClick={() => appWindow.toggleMaximize()} aria-label="Maximize" />
        </section>

        <section className="titlebar-drag">
          <nav className="titlebar-logo">
            <img src="/sion-icon.svg" alt="S-ION" />
            <span>S-ION</span>
          </nav>
        </section>

        <section className="titlebar-controls">
          <button className={`mode-badge ${mode}`} onClick={toggleMode} aria-label="Toggle mode">
            <span className="dot" />
            {mode === "smart" ? "Smart Mode" : "Expert Mode"}
          </button>
        </section>
      </header>

      <section className="content-wrapper">
        {/* ── Main Content ── */}
        <section className="main-content">
          <article className="hero">
            <img className="hero-icon" src="/sion-icon.svg" alt="S-ION" />
            <h1>What can <span>S-ION</span> do for you?</h1>
            <p>
              {mode === "smart"
                ? "Smart Mode — S-ION auto-routes to the best model for your task."
                : "Expert Mode — You control which model handles each task."}
            </p>

            {/* Expert Mode: Task Category Selector */}
            {mode === "expert" && (
              <nav className="category-selector">
                {TASK_CATEGORIES.map((cat) => (
                  <button
                    key={cat.key}
                    className={`category-chip ${selectedCategory === cat.key ? "active" : ""}`}
                    onClick={() => setSelectedCategory(cat.key)}
                  >
                    {cat.label}
                  </button>
                ))}
              </nav>
            )}

            {/* ── Action Bar ── */}
            <div className="action-bar">
              <input
                className="action-bar-input"
                type="text"
                placeholder={
                  mode === "smart"
                    ? "e.g. Summarize this article and find similar research..."
                    : `e.g. ${selectedCategory.replace(/_/g, " ")} task...`
                }
                value={intent}
                onChange={(e) => setIntent(e.target.value)}
                onKeyDown={handleKeyDown}
                autoFocus
              />
              <button
                className="action-bar-submit"
                onClick={handleSubmit}
                disabled={loading}
                aria-label="Submit"
              >
                {loading ? <span className="spinner" /> : (
                  <svg viewBox="0 0 24 24"><path d="M5 12h14M12 5l7 7-7 7" /></svg>
                )}
              </button>
            </div>

            {/* Shortcut Chips */}
            <nav className="shortcuts">
              <span className="shortcut-chip"><kbd>⌘</kbd><kbd>K</kbd> Action Bar</span>
              <span className="shortcut-chip"><kbd>⌘</kbd><kbd>E</kbd> Expert Mode</span>
              <span className="shortcut-chip"><kbd>⌘</kbd><kbd>.</kbd> Snap-Back</span>
            </nav>
          </article>

          {/* ── Smart Mode: Triage Result ── */}
          {smartResult?.triage && (
            <section className="triage-result">
              <header className="triage-header">
                <span className="triage-badge">⚡ TRIAGE</span>
                <span className="triage-model">Gemini Flash</span>
              </header>
              <p className="triage-reasoning">{smartResult.triage.reasoning}</p>
              <div className="triage-route">
                <span className="triage-label">Routed to:</span>
                <span className="triage-agent" style={{ color: AGENTS.find(a => a.key === smartResult.routed_to)?.color }}>
                  {AGENTS.find(a => a.key === smartResult.routed_to)?.emoji}{" "}
                  {smartResult.designation}
                </span>
                <span className="triage-confidence">
                  {Math.round(smartResult.triage.confidence * 100)}% confidence
                </span>
              </div>
            </section>
          )}

          {/* ── Expert Mode: Dispatch Result ── */}
          {smartResult && !smartResult.triage && smartResult.mode !== "fallback" && (
            <section className="triage-result expert-result">
              <header className="triage-header">
                <span className="triage-badge">📌 EXPERT PIN</span>
              </header>
              <div className="triage-route">
                <span className="triage-label">Pinned to:</span>
                <span className="triage-agent" style={{ color: AGENTS.find(a => a.key === smartResult.routed_to)?.color }}>
                  {AGENTS.find(a => a.key === smartResult.routed_to)?.emoji}{" "}
                  {smartResult.designation} ({smartResult.model_name})
                </span>
              </div>
            </section>
          )}

          {/* ── Pipeline (Commander + Audit) ── */}
          {pipeline && (
            <section className="pipeline-result">
              {pipeline.plan && (
                <article className="pipeline-stage">
                  <header className="stage-header commander">
                    <span className="stage-badge">🎯 COMMANDER</span>
                    <span className="stage-model">Kimi K2.5</span>
                  </header>
                  <p className="stage-reasoning">{pipeline.plan.reasoning}</p>
                  <ol className="execution-steps">
                    {pipeline.plan.steps.map((step) => (
                      <li key={step.step_id} className="execution-step">
                        <span className="step-agent">{step.agent.toUpperCase()}</span>
                        <span className="step-action">{step.action}</span>
                        {step.tool_calls.length > 0 && (
                          <span className="step-tools">
                            {step.tool_calls.map((t) => <code key={t}>{t}</code>)}
                          </span>
                        )}
                      </li>
                    ))}
                  </ol>
                </article>
              )}
              {pipeline.verdict && (
                <article className={`pipeline-stage ${pipeline.verdict.approved ? "approved" : "rejected"}`}>
                  <header className="stage-header audit">
                    <span className="stage-badge">
                      {pipeline.verdict.approved ? "✅" : "🚫"} AUDIT HOOK
                    </span>
                    <span className="stage-model">Opus 4.6</span>
                  </header>
                  <p className="stage-reasoning">{pipeline.verdict.reasoning}</p>
                  {pipeline.verdict.violations.length > 0 && (
                    <ul className="violations">
                      {pipeline.verdict.violations.map((v, i) => <li key={i} className="violation">{v}</li>)}
                    </ul>
                  )}
                </article>
              )}
              {pipeline.error && <output className="pipeline-error">⚠️ {pipeline.error}</output>}
            </section>
          )}

          {loading && (
            <output className="pipeline-loading">
              <span className="pulse">{mode === "smart" ? "⚡" : "📌"}</span>
              {mode === "smart" ? "Triage routing..." : "Expert dispatching..."}
            </output>
          )}

          {/* ── Swarm Status Bar ── */}
          <footer className="swarm-status">
            {AGENTS.map((agent) => (
              <span key={agent.key} className={`swarm-agent ${activeAgents.has(agent.key) ? "active" : ""}`}>
                <span className="indicator" />
                {agent.label}
              </span>
            ))}
          </footer>
        </section>

        {/* ── Expert Mode Sidebar (Mixing Board) ── */}
        {sidebarOpen && (
          <aside className="expert-sidebar">
            <h2 className="sidebar-title">Model Cockpit</h2>
            <p className="sidebar-subtitle">Pin models to task categories</p>

            <div className="pin-grid">
              {TASK_CATEGORIES.map((cat) => (
                <div key={cat.key} className="pin-card">
                  <label className="pin-label">{cat.label}</label>
                  <div className="pin-options">
                    {AGENTS.filter(a => a.key !== "audit_hook").map((agent) => (
                      <button
                        key={agent.key}
                        className={`pin-option ${pins[cat.key] === agent.key ? "pinned" : ""}`}
                        onClick={() => handlePinChange(cat.key, agent.key)}
                        title={agent.model}
                      >
                        <span className="pin-emoji">{agent.emoji}</span>
                        <span className="pin-model-name">{agent.model}</span>
                        {pins[cat.key] === agent.key && <span className="pin-active">📌</span>}
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>

            <div className="sidebar-info">
              <h3>Provider Health</h3>
              {AGENTS.map((agent) => (
                <div key={agent.key} className="health-row">
                  <span className="health-emoji">{agent.emoji}</span>
                  <span className="health-name">{agent.model}</span>
                  <div className="health-bar">
                    <div className="health-fill" style={{ width: "85%", background: agent.color }} />
                  </div>
                </div>
              ))}
            </div>
          </aside>
        )}
      </section>
    </main>
  );
}

export default App;
