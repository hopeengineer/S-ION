import { useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const appWindow = getCurrentWindow();

function App() {
  const [intent, setIntent] = useState("");
  const [mode, setMode] = useState<"smart" | "expert">("smart");
  const [routeResult, setRouteResult] = useState<string | null>(null);

  const handleSubmit = useCallback(async () => {
    if (!intent.trim()) return;
    try {
      const result = await invoke<string>("route_intent", { intent });
      setRouteResult(result);
    } catch (err) {
      console.error("Route error:", err);
    }
  }, [intent]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleSubmit();
  };

  const toggleMode = () => {
    setMode((m) => (m === "smart" ? "expert" : "smart"));
  };

  return (
    <main className="app-layout">
      {/* ── Custom Titlebar ── */}
      <header className="titlebar" data-tauri-drag-region>
        <section className="titlebar-traffic">
          <button
            className="traffic-light close"
            onClick={() => appWindow.close()}
            aria-label="Close"
          />
          <button
            className="traffic-light minimize"
            onClick={() => appWindow.minimize()}
            aria-label="Minimize"
          />
          <button
            className="traffic-light maximize"
            onClick={() => appWindow.toggleMaximize()}
            aria-label="Maximize"
          />
        </section>

        <section className="titlebar-drag">
          <nav className="titlebar-logo">
            <img src="/sion-icon.svg" alt="S-ION" />
            <span>S-ION</span>
          </nav>
        </section>

        <section className="titlebar-controls">
          <button
            className={`mode-badge ${mode}`}
            onClick={toggleMode}
            aria-label="Toggle mode"
          >
            <span className="dot" />
            {mode === "smart" ? "Smart Mode" : "Expert Mode"}
          </button>
        </section>
      </header>

      {/* ── Main Content ── */}
      <section className="main-content">
        <article className="hero">
          <img
            className="hero-icon"
            src="/sion-icon.svg"
            alt="S-ION"
          />
          <h1>
            What can <span>S-ION</span> do for you?
          </h1>
          <p>
            Your AI-native browser. Type a goal in plain English — S-ION's
            swarm intelligence handles the rest. Every action is previewed
            before execution.
          </p>

          {/* ── Natural Language Action Bar ── */}
          <div className="action-bar">
            <input
              className="action-bar-input"
              type="text"
              placeholder={
                mode === "smart"
                  ? "e.g. Organize my vacation photos and text the best one to my wife..."
                  : "e.g. Run cargo test --release && deploy to staging..."
              }
              value={intent}
              onChange={(e) => setIntent(e.target.value)}
              onKeyDown={handleKeyDown}
              autoFocus
            />
            <button
              className="action-bar-submit"
              onClick={handleSubmit}
              aria-label="Submit"
            >
              <svg viewBox="0 0 24 24">
                <path d="M5 12h14M12 5l7 7-7 7" />
              </svg>
            </button>
          </div>

          {/* ── Shortcut Chips ── */}
          <nav className="shortcuts">
            <span className="shortcut-chip">
              <kbd>⌘</kbd><kbd>K</kbd> Action Bar
            </span>
            <span className="shortcut-chip">
              <kbd>⌘</kbd><kbd>E</kbd> Expert Mode
            </span>
            <span className="shortcut-chip">
              <kbd>⌘</kbd><kbd>.</kbd> Snap-Back
            </span>
          </nav>
        </article>

        {/* ── Route Result (dev) ── */}
        {routeResult && (
          <output
            style={{
              marginTop: 16,
              padding: "8px 16px",
              background: "var(--sion-bg-secondary)",
              border: "1px solid var(--sion-border)",
              borderRadius: "var(--sion-radius-md)",
              fontFamily: "monospace",
              fontSize: 12,
              color: "var(--sion-text-secondary)",
            }}
          >
            Routed → {routeResult}
          </output>
        )}

        {/* ── Swarm Status Bar ── */}
        <footer className="swarm-status">
          <span className="swarm-agent active">
            <span className="indicator" />
            Conductor
          </span>
          <span className="swarm-agent">
            <span className="indicator" />
            Visionary
          </span>
          <span className="swarm-agent">
            <span className="indicator" />
            Builder
          </span>
          <span className="swarm-agent">
            <span className="indicator" />
            Scout
          </span>
        </footer>
      </section>
    </main>
  );
}

export default App;
