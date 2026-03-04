/**
 * S-ION CoPaw Bridge
 * 
 * Always-on Node.js gateway deployed on Railway.
 * Receives remote intents from WhatsApp/iMessage/Discord and
 * queues them for the local Tauri instance via WebSocket/SSE.
 * 
 * When the desktop app is asleep, messages queue in-memory.
 * On reconnect, the Tauri Rust backend calls resync to drain the queue.
 */

import { WebSocketServer } from "ws";
import express from "express";

const PORT = process.env.PORT || 3001;
const AUTH_TOKEN = process.env.COPAW_AUTH_TOKEN || "dev-token";

// In-memory message queue (production: use Redis/NATS)
const messageQueue = [];
let desktopSocket = null;

// ── Express server for health checks & SSE ──
const app = express();
app.use(express.json());

// Health check for Railway
app.get("/health", (_req, res) => {
    res.json({
        status: "ok",
        engine: "S-ION CoPaw Bridge",
        queue_depth: messageQueue.length,
        desktop_connected: desktopSocket !== null,
    });
});

// SSE endpoint (alternative to WebSocket for mobile clients)
app.get("/events", (req, res) => {
    const token = req.headers.authorization?.replace("Bearer ", "");
    if (token !== AUTH_TOKEN) {
        return res.status(401).json({ error: "Unauthorized" });
    }

    res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
    });

    // Drain queued messages
    while (messageQueue.length > 0) {
        const msg = messageQueue.shift();
        res.write(`data: ${JSON.stringify(msg)}\n\n`);
    }

    // Keep alive
    const keepAlive = setInterval(() => {
        res.write(":keepalive\n\n");
    }, 30000);

    req.on("close", () => clearInterval(keepAlive));
});

// Receive inbound messages from channel webhooks
app.post("/inbound/:channel", (req, res) => {
    const { channel } = req.params;
    const message = {
        id: crypto.randomUUID(),
        channel,
        payload: req.body,
        timestamp: new Date().toISOString(),
        delivered: false,
    };

    // If desktop is connected, send immediately
    if (desktopSocket && desktopSocket.readyState === 1) {
        desktopSocket.send(JSON.stringify(message));
        message.delivered = true;
    } else {
        // Queue for re-sync when desktop wakes
        messageQueue.push(message);
    }

    res.json({ queued: !message.delivered, id: message.id });
});

const server = app.listen(PORT, () => {
    console.log(`🌉 CoPaw Bridge listening on port ${PORT}`);
});

// ── WebSocket server for desktop Tauri connection ──
const wss = new WebSocketServer({ server, path: "/ws" });

wss.on("connection", (ws, req) => {
    const token = new URL(req.url, `http://localhost`).searchParams.get("token");
    if (token !== AUTH_TOKEN) {
        ws.close(4001, "Unauthorized");
        return;
    }

    console.log("🖥️  Desktop connected — draining queue...");
    desktopSocket = ws;

    // Re-sync: drain all queued messages
    while (messageQueue.length > 0) {
        const msg = messageQueue.shift();
        ws.send(JSON.stringify(msg));
    }

    ws.on("message", (data) => {
        // Handle responses from desktop (ACKs, outbound messages)
        console.log("📨 Desktop →", data.toString());
    });

    ws.on("close", () => {
        console.log("🖥️  Desktop disconnected — queuing messages");
        desktopSocket = null;
    });
});

console.log("🚀 S-ION CoPaw Bridge ready");
