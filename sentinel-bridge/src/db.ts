import Database from "better-sqlite3";
import path from "path";

/**
 * Initialize the SQLite database with both Sentinel and Dispatcher tables.
 * On Railway, use a mounted volume at /data/sion.db for persistence.
 */
export function initDatabase(): Database.Database {
    const dbPath = process.env.DB_PATH || path.join(__dirname, "..", "sion.db");
    const db = new Database(dbPath);

    // WAL mode for concurrent reads
    db.pragma("journal_mode = WAL");

    // Sentinel: scrubbed crash reports
    db.exec(`
    CREATE TABLE IF NOT EXISTS telemetry (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      install_id TEXT NOT NULL,
      app_version TEXT NOT NULL,
      event_type TEXT NOT NULL,
      error_code TEXT NOT NULL,
      logic_trace TEXT NOT NULL,
      model_used TEXT NOT NULL,
      agent_key TEXT NOT NULL,
      blocked_domain TEXT,
      event_ts TEXT NOT NULL,
      created_at TEXT DEFAULT (datetime('now'))
    )
  `);

    // Dispatcher: CoPaw message queue (Action Envelopes)
    db.exec(`
    CREATE TABLE IF NOT EXISTS missions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      source TEXT NOT NULL,
      sender TEXT NOT NULL,
      intent TEXT NOT NULL,
      payload TEXT,
      status TEXT DEFAULT 'pending',
      created_at TEXT DEFAULT (datetime('now')),
      claimed_at TEXT
    )
  `);

    console.log("📦 SQLite initialized at:", dbPath);
    return db;
}
