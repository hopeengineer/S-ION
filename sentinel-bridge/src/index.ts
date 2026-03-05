import express from "express";
import { initDatabase } from "./db";
import { authMiddleware } from "./auth";
import { sentinelRouter } from "./routes/sentinel";
import { bridgeRouter } from "./routes/bridge";

// ──────────────────────────────────────────────────
// S-ION Universal Bridge
// Sentinel (Telemetry) + Dispatcher (CoPaw) + Auth
// ──────────────────────────────────────────────────

const PORT = process.env.PORT || 3500;

// Initialize SQLite (Railway Volume at /data/sion.db)
const db = initDatabase();

const app = express();
app.use(express.json({ limit: "1mb" }));

// Health check (no auth required)
app.get("/health", (_req, res) => {
    res.json({
        status: "ok",
        service: "sion-bridge",
        version: "1.0.0",
        uptime: Math.floor(process.uptime()),
    });
});

// All other routes require the SION_BRIDGE_TOKEN
app.use(authMiddleware);

// Sentinel: telemetry ingestion + founder stats
app.use("/telemetry", sentinelRouter(db));

// Dispatcher: CoPaw-style message queue
app.use("/bridge", bridgeRouter(db));

// Start
app.listen(PORT, () => {
    console.log(`🌉 S-ION Universal Bridge running on port ${PORT}`);
    console.log(`   ── Sentinel Tunnel ──`);
    console.log(`   POST /telemetry          : Receive scrubbed crash reports`);
    console.log(`   GET  /telemetry/stats    : Founder's aggregate dashboard`);
    console.log(`   ── Dispatcher Tunnel ──`);
    console.log(`   POST /bridge/enqueue     : Queue a mission from WhatsApp/iMessage`);
    console.log(`   GET  /bridge/dequeue     : S-ION pulls pending missions`);
    console.log(`   GET  /bridge/pending     : Check queue depth`);
    console.log(`   ── Auth ──`);
    console.log(`   All routes require: Authorization: Bearer <SION_BRIDGE_TOKEN>`);
});
