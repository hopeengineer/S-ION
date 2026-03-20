import { useState, useEffect, useCallback } from "react";
import { commands } from "./bindings";
import "./MemoryBrowser.css";

// ── Types ──

interface MemoryEntry {
  id: string;
  content: string;
  category: string;
  source: string; // "global" or "project"
  score: number;
  created_at: number;
}

interface ProvisionStatus {
  ready: boolean;
  downloading: boolean;
  progress: number;
  model_name: string;
  error?: string;
}

// ── Category Colors ──
const CATEGORY_COLORS: Record<string, { bg: string; text: string; emoji: string }> = {
  preference: { bg: "rgba(139, 92, 246, 0.12)", text: "#8B5CF6", emoji: "💜" },
  architecture: { bg: "rgba(59, 130, 246, 0.12)", text: "#3B82F6", emoji: "🏗️" },
  gotcha: { bg: "rgba(239, 68, 68, 0.12)", text: "#EF4444", emoji: "⚠️" },
  pattern: { bg: "rgba(34, 197, 94, 0.12)", text: "#22C55E", emoji: "🔄" },
  fact: { bg: "rgba(234, 179, 8, 0.12)", text: "#EAB308", emoji: "📌" },
  style: { bg: "rgba(236, 72, 153, 0.12)", text: "#EC4899", emoji: "🎨" },
};

function getCategoryStyle(category: string) {
  return CATEGORY_COLORS[category] || { bg: "rgba(156, 163, 175, 0.12)", text: "#9CA3AF", emoji: "🧠" };
}

function timeAgo(timestamp: number): string {
  const now = Date.now() / 1000;
  const diff = now - timestamp;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export default function MemoryBrowser() {
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [provStatus, setProvStatus] = useState<ProvisionStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterTier, setFilterTier] = useState<"all" | "global" | "project">("all");
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  // Poll provisioning status
  useEffect(() => {
    const refresh = async () => {
      try {
        const res = await commands.memoryProvisionStatus();
        if (res.status === "ok") {
          setProvStatus(JSON.parse(res.data));
        }
      } catch { /* ignore */ }
    };
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => clearInterval(interval);
  }, []);

  // Load memories
  const loadMemories = useCallback(async () => {
    setLoading(true);
    try {
      const tier = filterTier === "all" ? null : filterTier;
      const res = await commands.memoryList(tier);
      if (res.status === "ok") {
        setMemories(JSON.parse(res.data));
      }
    } catch {
      /* ignore */
    } finally {
      setLoading(false);
    }
  }, [filterTier]);

  // Search memories
  const searchMemories = useCallback(async () => {
    if (!searchQuery.trim()) {
      loadMemories();
      return;
    }
    setLoading(true);
    try {
      const res = await commands.memoryQuery(searchQuery, 20);
      if (res.status === "ok") {
        setMemories(JSON.parse(res.data));
      }
    } catch {
      /* ignore */
    } finally {
      setLoading(false);
    }
  }, [searchQuery, loadMemories]);

  // Delete memory
  const handleDelete = async (id: string) => {
    setDeletingId(id);
    try {
      await commands.memoryDelete(id, false);
      setMemories((prev) => prev.filter((m) => m.id !== id));
    } catch {
      /* ignore */
    } finally {
      setDeletingId(null);
    }
  };

  // Start provisioning
  const handleProvision = async () => {
    try {
      await commands.memoryProvisionStart();
    } catch { /* ignore */ }
  };

  return (
    <div className="memory-browser">
      {/* Provisioning Banner */}
      {provStatus && !provStatus.ready && (
        <div className="memory-provision-banner">
          <div className="provision-header">
            <span className="provision-icon">
              {provStatus.downloading ? "⏳" : "🧠"}
            </span>
            <div className="provision-info">
              <span className="provision-title">
                {provStatus.downloading
                  ? "Downloading BGE-M3 INT8..."
                  : "Local AI Memory Not Ready"}
              </span>
              <span className="provision-subtitle">
                {provStatus.downloading
                  ? `${Math.round(provStatus.progress || 0)}% complete`
                  : "Using Gemini cloud fallback"}
              </span>
            </div>
          </div>
          {provStatus.downloading ? (
            <div className="provision-progress-track">
              <div
                className="provision-progress-fill"
                style={{ width: `${provStatus.progress || 0}%` }}
              />
            </div>
          ) : (
            <div className="provision-actions">
              <button className="provision-btn-download" onClick={handleProvision}>
                ⬇️ Download Model
              </button>
              <span className="provision-skip">
                ☁️ Cloud fallback active
              </span>
            </div>
          )}
        </div>
      )}

      {/* Search + Filters */}
      <div className="memory-controls">
        <div className="memory-search-row">
          <input
            type="text"
            className="memory-search"
            placeholder="Search memories semantically..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") searchMemories();
            }}
          />
          <button
            className="memory-search-btn"
            onClick={searchQuery.trim() ? searchMemories : loadMemories}
          >
            {searchQuery.trim() ? "🔍" : "↻"}
          </button>
        </div>
        <div className="memory-filters">
          {(["all", "global", "project"] as const).map((tier) => (
            <button
              key={tier}
              className={`memory-filter ${filterTier === tier ? "active" : ""}`}
              onClick={() => setFilterTier(tier)}
            >
              {tier === "all" ? "🌐 All" : tier === "global" ? "🏠 Global" : "📁 Project"}
            </button>
          ))}
        </div>
      </div>

      {/* Memory Count */}
      <div className="memory-count">
        {loading ? (
          <span className="memory-count-loading">Loading...</span>
        ) : (
          <span>{memories.length} brain cell{memories.length !== 1 ? "s" : ""}</span>
        )}
      </div>

      {/* Memory List */}
      <div className="memory-list">
        {memories.length === 0 && !loading && (
          <div className="memory-empty">
            <span className="memory-empty-icon">🧠</span>
            <p>No memories yet.</p>
            <p className="memory-empty-hint">
              S-ION will automatically learn from your conversations.
            </p>
          </div>
        )}

        {memories.map((mem) => {
          const catStyle = getCategoryStyle(mem.category);
          const isExpanded = expandedId === mem.id;
          return (
            <div key={mem.id} className={`memory-card ${isExpanded ? "expanded" : ""}`}>
              <div
                className="memory-card-header"
                onClick={() => setExpandedId(isExpanded ? null : mem.id)}
              >
                <div className="memory-card-meta">
                  <span
                    className="memory-category-badge"
                    style={{ background: catStyle.bg, color: catStyle.text }}
                  >
                    {catStyle.emoji} {mem.category}
                  </span>
                  <span className="memory-tier-badge">
                    {mem.source === "global" ? "🏠" : "📁"}
                  </span>
                </div>
                <p className="memory-card-preview">
                  {isExpanded ? mem.content : mem.content.slice(0, 120) + (mem.content.length > 120 ? "..." : "")}
                </p>
                <div className="memory-card-footer">
                  <span className="memory-time">{timeAgo(mem.created_at)}</span>
                  {mem.score > 0 && (
                    <span className="memory-score">
                      {Math.round(mem.score * 100)}% match
                    </span>
                  )}
                </div>
              </div>

              {isExpanded && (
                <div className="memory-card-actions">
                  <button
                    className="memory-delete-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(mem.id);
                    }}
                    disabled={deletingId === mem.id}
                  >
                    {deletingId === mem.id ? "Deleting..." : "🗑️ Delete"}
                  </button>
                  <button
                    className="memory-copy-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      navigator.clipboard.writeText(mem.content);
                    }}
                  >
                    📋 Copy
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
