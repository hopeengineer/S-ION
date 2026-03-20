import { useState, useEffect, useCallback } from "react";
import { commands } from "./bindings";
import "./SidecarMonitor.css";

// ── Types ──

interface SidecarStatus {
  platform: string;
  status: string;
  temperature: "cold" | "warm" | "hot";
  needs_provisioning: boolean;
  is_running: boolean;
}

interface SidecarHealth {
  type: string;
  cpu_percent: number;
  memory_used_mb: number;
  memory_limit_mb: number;
  uptime_secs: number;
  snapback_ready: boolean;
  workspace_files: number;
}

// ── Helpers ──

function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
}

function getMemoryPercent(used: number, limit: number): number {
  if (limit === 0) return 0;
  return Math.min(100, Math.round((used / limit) * 100));
}

// ── Component ──

export default function SidecarMonitor() {
  const [status, setStatus] = useState<SidecarStatus | null>(null);
  const [health, setHealth] = useState<SidecarHealth | null>(null);
  const [provisioningMsg, setProvisioningMsg] = useState<string>("");

  // Poll sidecar status every 2 seconds
  const pollStatus = useCallback(async () => {
    try {
      const raw = await commands.sidecarStatus();
      const parsed: SidecarStatus = JSON.parse(raw);
      setStatus(parsed);
    } catch {
      // Silently fail — backend may not be ready
    }
  }, []);

  // Poll health every 3 seconds (only when running)
  const pollHealth = useCallback(async () => {
    try {
      const raw = await commands.sidecarHealth();
      if (raw && raw !== "null") {
        const parsed: SidecarHealth = JSON.parse(raw);
        setHealth(parsed);
      }
    } catch {
      // No health data yet
    }
  }, []);

  useEffect(() => {
    pollStatus();
    const statusInterval = setInterval(pollStatus, 2000);
    const healthInterval = setInterval(pollHealth, 3000);
    return () => {
      clearInterval(statusInterval);
      clearInterval(healthInterval);
    };
  }, [pollStatus, pollHealth]);

  // Actions
  const handleProvision = async () => {
    try {
      const res = await commands.sidecarProvision();
      if (res.status === "error") throw new Error(res.error);
      setProvisioningMsg(res.data);
      pollStatus();
    } catch (e) {
      setProvisioningMsg(`Error: ${e}`);
    }
  };

  const handleBoot = async () => {
    try {
      const res = await commands.sidecarBoot();
      if (res.status === "error") throw new Error(res.error);
      pollStatus();
    } catch {
      // Handle error
    }
  };

  const handleShutdown = async () => {
    try {
      const res = await commands.sidecarShutdown();
      if (res.status === "error") throw new Error(res.error);
      setHealth(null);
      pollStatus();
    } catch {
      // Handle error
    }
  };

  if (!status) return null;

  const temp = status.temperature;
  const memPct = health ? getMemoryPercent(health.memory_used_mb, health.memory_limit_mb) : 0;
  const memWarning = memPct >= 90;

  return (
    <div className={`sidecar-monitor sidecar-${temp}`}>
      {/* Header: Status + Platform */}
      <div className="sidecar-header">
        <div className="sidecar-indicator">
          <span className={`sidecar-dot sidecar-dot-${temp}`} />
          <span className="sidecar-label">{status.status}</span>
        </div>
        <span className="sidecar-platform">{status.platform}</span>
      </div>

      {/* Provisioning CTA */}
      {status.needs_provisioning && (
        <div className="sidecar-provision">
          <p className="sidecar-provision-text">
            🔒 VM kernel needed for Expert isolation.
            Your code runs in a sealed environment — no network, no host access.
          </p>
          <button className="sidecar-provision-btn" onClick={handleProvision}>
            Install Sidecar
          </button>
          {provisioningMsg && (
            <p className="sidecar-provision-msg">{provisioningMsg}</p>
          )}
        </div>
      )}

      {/* Boot/Shutdown Controls */}
      {!status.needs_provisioning && !status.is_running && status.status !== "Not Needed" && (
        <button className="sidecar-boot-btn" onClick={handleBoot}>
          ⚡ Boot Sidecar
        </button>
      )}
      {status.is_running && (
        <button className="sidecar-shutdown-btn" onClick={handleShutdown}>
          ■ Shut Down
        </button>
      )}

      {/* Health Telemetry (only when we have data) */}
      {health && (
        <div className="sidecar-telemetry">
          {/* Memory Bar */}
          <div className="sidecar-metric">
            <div className="sidecar-metric-header">
              <span className="sidecar-metric-label">Memory</span>
              <span className={`sidecar-metric-value ${memWarning ? "sidecar-warning" : ""}`}>
                {health.memory_used_mb}MB / {health.memory_limit_mb}MB
              </span>
            </div>
            <div className="sidecar-bar">
              <div
                className={`sidecar-bar-fill ${memWarning ? "sidecar-bar-warning" : ""}`}
                style={{ width: `${memPct}%` }}
              />
            </div>
          </div>

          {/* CPU */}
          <div className="sidecar-metric">
            <div className="sidecar-metric-header">
              <span className="sidecar-metric-label">CPU</span>
              <span className="sidecar-metric-value">
                {health.cpu_percent.toFixed(1)}%
              </span>
            </div>
            <div className="sidecar-bar">
              <div
                className="sidecar-bar-fill sidecar-bar-cpu"
                style={{ width: `${Math.min(100, health.cpu_percent)}%` }}
              />
            </div>
          </div>

          {/* Bottom row: Uptime + Files + Snap-Back */}
          <div className="sidecar-stats">
            <span className="sidecar-stat" title="Uptime">
              ⏱ {formatUptime(health.uptime_secs)}
            </span>
            <span className="sidecar-stat" title="Workspace files">
              📁 {health.workspace_files}
            </span>
            <span
              className={`sidecar-snapback ${health.snapback_ready ? "ready" : "not-ready"}`}
              title={health.snapback_ready ? "Snap-Back ready — safe to experiment" : "Snap-Back not available"}
            >
              {health.snapback_ready ? "🛡️" : "⚠️"} Snap-Back
            </span>
          </div>
        </div>
      )}

      {/* Smart Mode indicator */}
      {status.status === "Not Needed" && (
        <p className="sidecar-smart-msg">
          sandbox-exec active. No VM overhead.
        </p>
      )}
    </div>
  );
}
