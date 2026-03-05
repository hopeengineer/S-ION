import { Router } from "express";
import Database from "better-sqlite3";

/**
 * Dispatcher Tunnel (CoPaw-Equivalent): Message Queue for Remote AI Intents.
 *
 * Flow:
 *   1. Grandma texts WhatsApp → webhook hits POST /bridge/enqueue
 *   2. S-ION desktop wakes up → calls GET /bridge/dequeue
 *   3. S-ION processes the mission → result sent back to Grandma
 *
 * Missions are "Action Envelopes": they contain the intent but no secrets.
 * Long-term memory stays in local LanceDB, not here.
 */
export function bridgeRouter(db: Database.Database): Router {
    const router = Router();

    const enqueueStmt = db.prepare(`
    INSERT INTO missions (source, sender, intent, payload)
    VALUES (@source, @sender, @intent, @payload)
  `);

    const dequeueStmt = db.prepare(`
    UPDATE missions SET status = 'claimed', claimed_at = datetime('now')
    WHERE id = (SELECT id FROM missions WHERE status = 'pending' ORDER BY created_at ASC LIMIT 1)
    RETURNING *
  `);

    // Enqueue a new mission (from WhatsApp webhook, iMessage bridge, etc.)
    router.post("/enqueue", (req, res) => {
        const { source, sender, intent, payload } = req.body;

        if (!source || !sender || !intent) {
            res.status(400).json({ error: "Missing required fields: source, sender, intent" });
            return;
        }

        try {
            const result = enqueueStmt.run({
                source: source,
                sender: sender,
                intent: intent,
                payload: payload ? JSON.stringify(payload) : null,
            });
            console.log(`📨 Mission queued: [${source}] ${sender}: "${intent.slice(0, 50)}..."`);
            res.json({ status: "queued", id: result.lastInsertRowid });
        } catch (err: any) {
            console.error("❌ Enqueue error:", err.message);
            res.status(500).json({ error: "Failed to queue mission" });
        }
    });

    // Dequeue the next pending mission (S-ION Heartbeat pulls this)
    router.get("/dequeue", (_req, res) => {
        try {
            const mission = dequeueStmt.get() as any;
            if (!mission) {
                res.json({ status: "empty", mission: null });
                return;
            }
            console.log(`🚀 Mission claimed: #${mission.id} from ${mission.source}`);
            res.json({ status: "claimed", mission });
        } catch (err: any) {
            console.error("❌ Dequeue error:", err.message);
            res.status(500).json({ error: "Failed to dequeue mission" });
        }
    });

    // Check how many missions are pending
    router.get("/pending", (_req, res) => {
        try {
            const result = db.prepare("SELECT COUNT(*) as count FROM missions WHERE status = 'pending'").get() as any;
            res.json({ pending: result.count });
        } catch (err: any) {
            res.status(500).json({ error: "Failed to check queue" });
        }
    });

    return router;
}
