import { Router } from "express";
import Database from "better-sqlite3";

/**
 * Sentinel Tunnel: Privacy-preserving telemetry ingestion.
 * POST /telemetry        : Receive scrubbed crash reports from S-ION clients
 * GET  /telemetry/stats  : Founder's aggregate dashboard data
 */
export function sentinelRouter(db: Database.Database): Router {
    const router = Router();

    const insertStmt = db.prepare(`
    INSERT INTO telemetry (install_id, app_version, event_type, error_code, logic_trace, model_used, agent_key, blocked_domain, event_ts)
    VALUES (@install_id, @app_version, @event_type, @error_code, @logic_trace, @model_used, @agent_key, @blocked_domain, @timestamp)
  `);

    // Receive a scrubbed report
    router.post("/", (req, res) => {
        const report = req.body;

        const required = ["install_id", "app_version", "event_type", "error_code", "logic_trace", "model_used", "agent_key", "timestamp"];
        const missing = required.filter((f) => !(f in report));
        if (missing.length > 0) {
            res.status(400).json({ error: `Missing fields: ${missing.join(", ")}` });
            return;
        }

        try {
            insertStmt.run({
                install_id: report.install_id,
                app_version: report.app_version,
                event_type: report.event_type,
                error_code: report.error_code,
                logic_trace: report.logic_trace,
                model_used: report.model_used,
                agent_key: report.agent_key,
                blocked_domain: report.blocked_domain ?? null,
                timestamp: report.timestamp,
            });
            console.log(`🔭 Report: [${report.event_type}] ${report.error_code} from ${report.agent_key}`);
            res.json({ status: "received" });
        } catch (err: any) {
            console.error("❌ Sentinel insert error:", err.message);
            res.status(500).json({ error: "Failed to store report" });
        }
    });

    // Founder's aggregate stats
    router.get("/stats", (_req, res) => {
        try {
            const total = db.prepare("SELECT COUNT(*) as count FROM telemetry").get() as any;
            const installs = db.prepare("SELECT COUNT(DISTINCT install_id) as count FROM telemetry").get() as any;
            const byType = db.prepare("SELECT event_type, COUNT(*) as count FROM telemetry GROUP BY event_type ORDER BY count DESC").all();
            const byModel = db.prepare("SELECT model_used, COUNT(*) as count FROM telemetry GROUP BY model_used ORDER BY count DESC").all();
            const byAgent = db.prepare("SELECT agent_key, COUNT(*) as count FROM telemetry GROUP BY agent_key ORDER BY count DESC").all();
            const topBlocked = db.prepare("SELECT blocked_domain, COUNT(*) as count FROM telemetry WHERE blocked_domain IS NOT NULL GROUP BY blocked_domain ORDER BY count DESC LIMIT 10").all();
            const recent = db.prepare("SELECT * FROM telemetry ORDER BY created_at DESC LIMIT 20").all();

            res.json({
                total_reports: total.count,
                unique_installs: installs.count,
                by_event_type: byType,
                by_model: byModel,
                by_agent: byAgent,
                top_blocked_domains: topBlocked,
                recent_reports: recent,
            });
        } catch (err: any) {
            console.error("❌ Stats query error:", err.message);
            res.status(500).json({ error: "Failed to query stats" });
        }
    });

    return router;
}
