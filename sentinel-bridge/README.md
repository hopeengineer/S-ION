# S-ION Universal Bridge

The **Sovereign Switchboard** for S-ION: privacy-preserving telemetry + CoPaw-style message queue.

## Architecture

| Tunnel | Endpoint | Purpose |
|---|---|---|
| **Sentinel** | `POST /telemetry` | Receive PII-scrubbed crash reports |
| **Sentinel** | `GET /telemetry/stats` | Founder's aggregate dashboard |
| **Dispatcher** | `POST /bridge/enqueue` | Queue missions from WhatsApp/iMessage |
| **Dispatcher** | `GET /bridge/dequeue` | S-ION Heartbeat claims next mission |
| **Dispatcher** | `GET /bridge/pending` | Check queue depth |
| **Health** | `GET /health` | Railway health check |

## Environment Variables

| Variable | Required | Description |
|---|---|---|
| `PORT` | No | Server port (default: 3500) |
| `SION_BRIDGE_TOKEN` | Yes | Shared secret for auth |
| `DB_PATH` | No | SQLite path (default: /data/sion.db) |

## Deploy to Railway

1. Push the `sentinel-bridge/` directory to a GitHub repo
2. Create a new Railway service linked to that repo
3. Set `SION_BRIDGE_TOKEN` in Railway env vars
4. Add a **Volume** mounted at `/data`
5. Deploy: Railway builds the Dockerfile automatically
6. Copy the Railway URL into `SAM_LOGIC.yaml` -> `privacy.sentinel.railway_endpoint`

## Local Dev

```bash
cd sentinel-bridge
npm install
npm run dev
# Test:
curl http://localhost:3500/health
```
