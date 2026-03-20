import { useState, useCallback, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import SidecarMonitor from "./SidecarMonitor";

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

interface SecurityEvent {
  timestamp: string;
  domain: string;
  full_url: string;
  status: string;
  agent_key: string;
}

interface SentinelReport {
  install_id: string;
  app_version: string;
  event_type: string;
  error_code: string;
  logic_trace: string;
  model_used: string;
  agent_key: string;
  blocked_domain: string | null;
  timestamp: string;
}

interface FileChange {
  status: string;
  before: string | null;
  after: string | null;
}

interface SandboxResult {
  execution_id: string;
  agent_key: string;
  command: string;
  stdout: string;
  stderr: string;
  exit_code: number;
  duration_ms: number;
  timed_out: boolean;
  file_changes: Record<string, FileChange>;
  snapshot_id: string;
}

// ── Model Registry (for Expert Mode UI) ──

const AGENTS = [
  { key: "commander", label: "Commander", model: "Kimi K2.5", emoji: "🎯", color: "#FF4500" },
  { key: "audit_hook", label: "Audit Hook", model: "Claude 4.6", emoji: "🛡️", color: "#6B6B6B" },
  { key: "analyst", label: "Analyst", model: "DeepSeek 3.2", emoji: "🔍", color: "#4A90D9" },
  { key: "visionary", label: "Visionary", model: "Gemini 3.1 Pro", emoji: "👁️", color: "#34A853" },
  { key: "builder", label: "Builder", model: "Claude 4.6", emoji: "🔨", color: "#9B59B6" },
  { key: "scout", label: "Scout", model: "GPT-5 Mini", emoji: "🏃", color: "#F39C12" },
  { key: "fast_designer", label: "Fast Designer", model: "Nano Banana", emoji: "🍌", color: "#FFD700" },
  { key: "pro_designer", label: "Pro Designer", model: "Nano Banana Pro", emoji: "🎨", color: "#FF8C00" },
];

const TASK_CATEGORIES = [
  { key: "terminal_cli", label: "Terminal / CLI" },
  { key: "frontend_coding", label: "Frontend Coding" },
  { key: "logic_auditing", label: "Logic Auditing" },
  { key: "web_research", label: "Web Research" },
  { key: "grandma_mode_ui", label: "Grandma Mode" },
];

function GrandmaError({ error }: { error: string }) {
  const [showDetails, setShowDetails] = useState(false);

  const parts = error.split("\n\n[Dev Details: ");
  const grandmaMessage = parts[0];
  const devDetails = parts.length > 1 ? parts[1].replace(/\]$/, "") : null;

  return (
    <output className="grandma-error">
      <div className="grandma-message">
        <span className="grandma-icon">👵</span>
        <p>{grandmaMessage}</p>
      </div>
      {devDetails && (
        <div className="grandma-dev-toggle">
          <button onClick={() => setShowDetails(!showDetails)}>
            {showDetails ? "Hide Technical Details" : "View Technical Details"}
          </button>
          {showDetails && <pre className="dev-details">{devDetails}</pre>}
        </div>
      )}
    </output>
  );
}

function App() {
  const [intent, setIntent] = useState("");
  const [mode, setMode] = useState<"smart" | "expert">("smart");
  const [loading, setLoading] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">(
    () => (localStorage.getItem("sion-theme") as "light" | "dark") || "light"
  );

  // Smart Mode state
  const [smartResult, setSmartResult] = useState<DispatchResult | null>(null);
  const [pipeline, setPipeline] = useState<PipelineResult | null>(null);

  // Expert Mode state
  const [pins, setPins] = useState<Record<string, string>>({});
  const [selectedCategory, setSelectedCategory] = useState("terminal_cli");

  // Sidebar tab state
  const [sidebarTab, setSidebarTab] = useState<"cockpit" | "security" | "sentinel">("cockpit");

  // Security Dashboard state
  const [securityLog, setSecurityLog] = useState<SecurityEvent[]>([]);

  // Sentinel Consent Toast state
  const [pendingReport, setPendingReport] = useState<SentinelReport | null>(null);
  const [showReportJson, setShowReportJson] = useState(false);

  // Founder check
  const [isFounder, setIsFounder] = useState(false);

  // Sandbox Action Card state
  const [actionCard, setActionCard] = useState<SandboxResult | null>(null);
  const [actionCardExpanded, setActionCardExpanded] = useState(false);

  // Apply theme to document
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("sion-theme", theme);
  }, [theme]);

  // Load pins from Rust backend on mount
  useEffect(() => {
    invoke<string>("get_model_pins").then((res) => {
      try { setPins(JSON.parse(res)); } catch { }
    });
    // Check founder status
    invoke<boolean>("is_founder").then(setIsFounder).catch(() => { });
  }, []);

  // Ctrl+Shift+S: toggle hidden Sentinel tab
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key === "S") {
        e.preventDefault();
        if (isFounder && sidebarOpen) {
          setSidebarTab((prev) => (prev === "sentinel" ? "cockpit" : "sentinel"));
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isFounder, sidebarOpen]);

  // Poll security log and pending reports when security tab is active
  useEffect(() => {
    if (!sidebarOpen) return;
    const poll = setInterval(() => {
      invoke<string>("get_security_log").then((res) => {
        try { setSecurityLog(JSON.parse(res)); } catch { }
      });
      invoke<string>("get_pending_report").then((res) => {
        try {
          const parsed = JSON.parse(res);
          setPendingReport(parsed || null);
        } catch { setPendingReport(null); }
      });
    }, 2000);
    return () => clearInterval(poll);
  }, [sidebarOpen]);

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

  const toggleTheme = () => {
    setTheme((prev) => (prev === "light" ? "dark" : "light"));
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
          <button className="theme-toggle" onClick={toggleTheme} aria-label="Toggle theme" title={theme === "light" ? "Switch to dark mode" : "Switch to light mode"}>
            {theme === "light" ? (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" /></svg>
            ) : (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="5" /><line x1="12" y1="1" x2="12" y2="3" /><line x1="12" y1="21" x2="12" y2="23" /><line x1="4.22" y1="4.22" x2="5.64" y2="5.64" /><line x1="18.36" y1="18.36" x2="19.78" y2="19.78" /><line x1="1" y1="12" x2="3" y2="12" /><line x1="21" y1="12" x2="23" y2="12" /><line x1="4.22" y1="19.78" x2="5.64" y2="18.36" /><line x1="18.36" y1="5.64" x2="19.78" y2="4.22" /></svg>
            )}
          </button>
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
                ? "Smart Mode: S-ION auto-routes to the best model for your task."
                : "Expert Mode: You control which model handles each task."}
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

          {smartResult?.error && <GrandmaError error={smartResult.error} />}

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
              {pipeline.error && <GrandmaError error={pipeline.error} />}
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

        {/* Expert Mode Sidebar */}
        {sidebarOpen && (
          <aside className="expert-sidebar">
            {/* Tab Switcher */}
            <nav className="sidebar-tabs">
              <button
                className={`sidebar-tab ${sidebarTab === "cockpit" ? "active" : ""}`}
                onClick={() => setSidebarTab("cockpit")}
              >
                🏛️ Cockpit
              </button>
              <button
                className={`sidebar-tab ${sidebarTab === "security" ? "active" : ""}`}
                onClick={() => setSidebarTab("security")}
              >
                🛡️ Security
              </button>
            </nav>

            {/* Cockpit Tab */}
            {sidebarTab === "cockpit" && (
              <>
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
              </>
            )}

            {/* Security Tab */}
            {sidebarTab === "security" && (
              <>
                <div className="security-header">
                  <div>
                    <h2 className="sidebar-title">Security Dashboard</h2>
                    <p className="sidebar-subtitle">Real-time egress monitoring</p>
                  </div>
                  {securityLog.length > 0 && (
                    <button
                      className="copy-log-btn"
                      onClick={() => {
                        navigator.clipboard.writeText(JSON.stringify(securityLog, null, 2));
                        const btn = document.querySelector(".copy-log-btn") as HTMLButtonElement;
                        if (btn) { btn.textContent = "Copied!"; setTimeout(() => btn.textContent = "📋 Copy Log", 1500); }
                      }}
                    >
                      📋 Copy Log
                    </button>
                  )}
                </div>

                <div className="security-log">
                  {securityLog.length === 0 ? (
                    <p className="security-empty">🛡️ No network events yet. Submit a query to see the egress log.</p>
                  ) : (
                    securityLog.slice().reverse().map((evt, i) => (
                      <div key={i} className={`security-event ${evt.status}`}>
                        <span className="security-status">
                          {evt.status === "pass" ? "✅" : "🚫"}
                        </span>
                        <div className="security-info">
                          <span className="security-domain">{evt.domain}</span>
                          <span className="security-agent">{evt.agent_key}</span>
                        </div>
                        <span className="security-time">
                          {new Date(evt.timestamp).toLocaleTimeString()}
                        </span>
                      </div>
                    ))
                  )}
                </div>
              </>
            )}

            {/* Sentinel Tab (Founder Only, Ctrl+Shift+S) */}
            {sidebarTab === "sentinel" && isFounder && (
              <>
                <h2 className="sidebar-title">🔭 Sentinel</h2>
                <p className="sidebar-subtitle">Founder's telemetry view</p>
                <div className="sentinel-placeholder">
                  <p>Aggregate error data from Railway will appear here once the endpoint is configured.</p>
                </div>
              </>
            )}

            {/* Sidecar Monitor — always visible at sidebar bottom */}
            <SidecarMonitor />
          </aside>
        )}

        {/* Consent Toast */}
        {pendingReport && (
          <div className="consent-toast">
            <div className="consent-header">
              <span className="consent-icon">⚠️</span>
              <p>Oops! S-ION hit a bump. Can we send a masked report to Sam so he can fix it for you?</p>
            </div>
            <div className="consent-actions">
              <button
                className="consent-view"
                onClick={() => setShowReportJson(!showReportJson)}
              >
                {showReportJson ? "Hide Report" : "View Report"}
              </button>
              <button
                className="consent-send"
                onClick={async () => {
                  await invoke("approve_report");
                  setPendingReport(null);
                  setShowReportJson(false);
                }}
              >
                Send
              </button>
              <button
                className="consent-dismiss"
                onClick={async () => {
                  await invoke("dismiss_report");
                  setPendingReport(null);
                  setShowReportJson(false);
                }}
              >
                No Thanks
              </button>
            </div>
            {showReportJson && (
              <div className="consent-json-wrap">
                <button
                  className="copy-json-btn"
                  onClick={() => {
                    navigator.clipboard.writeText(JSON.stringify(pendingReport, null, 2));
                    const btn = document.querySelector(".copy-json-btn") as HTMLButtonElement;
                    if (btn) { btn.textContent = "Copied!"; setTimeout(() => btn.textContent = "📋 Copy", 1500); }
                  }}
                >
                  📋 Copy
                </button>
                <pre className="consent-json">{JSON.stringify(pendingReport, null, 2)}</pre>
              </div>
            )}
          </div>
        )}

        {/* Action Card: Sandbox Execution Preview */}
        {actionCard && (
          <div className="action-card">
            <div className="action-card-header">
              <div className="action-card-title">
                <span className="action-card-icon">🏗️</span>
                <div>
                  <h3>Sandbox Execution Complete</h3>
                  <p className="action-card-agent">
                    Agent: <strong>{actionCard.agent_key}</strong> |
                    Exit: <span className={actionCard.exit_code === 0 ? "exit-ok" : "exit-fail"}>{actionCard.exit_code}</span> |
                    {actionCard.duration_ms}ms
                    {actionCard.timed_out && <span className="exit-fail"> (TIMED OUT)</span>}
                  </p>
                </div>
              </div>
              <button
                className="action-card-toggle"
                onClick={() => setActionCardExpanded(!actionCardExpanded)}
              >
                {actionCardExpanded ? "▲ Collapse" : "▼ View Changes"}
              </button>
            </div>

            {actionCardExpanded && (
              <div className="action-card-body">
                {/* stdout/stderr */}
                {actionCard.stdout && (
                  <div className="action-card-output">
                    <label>Output:</label>
                    <pre>{actionCard.stdout}</pre>
                  </div>
                )}
                {actionCard.stderr && (
                  <div className="action-card-output action-card-stderr">
                    <label>Errors:</label>
                    <pre>{actionCard.stderr}</pre>
                  </div>
                )}

                {/* File Diff Viewer */}
                {Object.keys(actionCard.file_changes).length > 0 && (
                  <div className="action-card-diffs">
                    <label>File Changes ({Object.keys(actionCard.file_changes).length}):</label>
                    {Object.entries(actionCard.file_changes).map(([name, change]) => (
                      <div key={name} className={`action-diff ${change.status}`}>
                        <div className="action-diff-header">
                          <span className="action-diff-status">
                            {change.status === "added" ? "+" : change.status === "deleted" ? "-" : "~"}
                          </span>
                          <span className="action-diff-name">{name}</span>
                          <span className="action-diff-label">{change.status}</span>
                        </div>
                        {change.before && (
                          <pre className="action-diff-before">- {change.before.slice(0, 500)}</pre>
                        )}
                        {change.after && (
                          <pre className="action-diff-after">+ {change.after.slice(0, 500)}</pre>
                        )}
                      </div>
                    ))}
                  </div>
                )}

                <button
                  className="action-card-copy"
                  onClick={() => {
                    navigator.clipboard.writeText(JSON.stringify(actionCard, null, 2));
                    const btn = document.querySelector(".action-card-copy") as HTMLButtonElement;
                    if (btn) { btn.textContent = "Copied!"; setTimeout(() => btn.textContent = "📋 Copy Full Result", 1500); }
                  }}
                >
                  📋 Copy Full Result
                </button>
              </div>
            )}

            <div className="action-card-actions">
              <button
                className="action-apply"
                onClick={async () => {
                  try {
                    await invoke("sandbox_apply", { executionId: actionCard.execution_id, targetDir: "." });
                    setActionCard(null);
                    setActionCardExpanded(false);
                  } catch (err) {
                    console.error("Apply failed:", err);
                  }
                }}
              >
                ✅ Apply Changes
              </button>
              <button
                className="action-snapback"
                onClick={async () => {
                  try {
                    await invoke("sandbox_snapback", { snapshotId: actionCard.snapshot_id });
                    setActionCard(null);
                    setActionCardExpanded(false);
                  } catch (err) {
                    console.error("Snap-Back failed:", err);
                  }
                }}
              >
                ⏪ Snap-Back
              </button>
            </div>
          </div>
        )}
      </section>
    </main>
  );
}

export default App;
