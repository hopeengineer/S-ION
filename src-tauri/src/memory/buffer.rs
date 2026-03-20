use rusqlite::{params, Connection as SqliteConn};
use std::path::PathBuf;
use std::sync::Mutex;

// ──────────────────────────────────────────────────
// Dreaming Buffer (SQLite)
// ──────────────────────────────────────────────────

/// Temporary storage for memories captured while the ONNX model is downloading.
/// Once the model is ready, a background "Dreaming" task flushes these to LanceDB.
pub struct DreamBuffer {
    conn: Mutex<SqliteConn>,
    path: PathBuf,
}

/// A buffered memory waiting to be promoted to vector storage.
#[derive(Debug, Clone)]
pub struct BufferedMemory {
    pub id: i64,
    pub content: String,
    pub category: String,
    pub is_global: bool,
    pub created_at: i64,
    pub metadata: String,
}

impl DreamBuffer {
    /// Get the path to the dream buffer database.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Initialize the dreaming buffer at the OS-native data directory.
    pub fn init() -> Result<Self, String> {
        let data_dir = dirs::data_local_dir().ok_or("Cannot determine OS data directory")?;
        let sion_dir = data_dir.join("com.s-ion.dev");
        std::fs::create_dir_all(&sion_dir)
            .map_err(|e| format!("Failed to create S-ION data dir: {}", e))?;

        let db_path = sion_dir.join("dream_buffer.db");
        let conn = SqliteConn::open(&db_path)
            .map_err(|e| format!("Failed to open dream buffer: {}", e))?;

        // WAL mode for crash safety
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("WAL mode failed: {}", e))?;

        // Create table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dreams (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'observation',
                is_global INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                promoted INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .map_err(|e| format!("Table creation failed: {}", e))?;

        println!("💤 Dream Buffer initialized: {}", db_path.display());

        Ok(Self {
            conn: Mutex::new(conn),
            path: db_path,
        })
    }

    /// Save a memory to the buffer (while ONNX model is downloading).
    pub fn save(
        &self,
        content: &str,
        category: &str,
        is_global: bool,
        metadata: &str,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO dreams (content, category, is_global, created_at, metadata) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![content, category, is_global as i32, now, metadata],
        ).map_err(|e| format!("Insert failed: {}", e))?;

        let id = conn.last_insert_rowid();
        println!(
            "💤 Buffered dream #{}: [{}] {}",
            id,
            category,
            &content[..content.len().min(50)]
        );
        Ok(id)
    }

    /// Get all unpromoted memories (for flushing to LanceDB).
    pub fn get_unpromoted(&self) -> Result<Vec<BufferedMemory>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, content, category, is_global, created_at, metadata FROM dreams WHERE promoted = 0 ORDER BY id"
        ).map_err(|e| format!("Prepare failed: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(BufferedMemory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    is_global: row.get::<_, i32>(3)? != 0,
                    created_at: row.get(4)?,
                    metadata: row.get(5)?,
                })
            })
            .map_err(|e| format!("Query failed: {}", e))?;

        let mut result = Vec::new();
        for row in rows {
            if let Ok(mem) = row {
                result.push(mem);
            }
        }

        Ok(result)
    }

    /// Mark a specific dream as promoted (after successful LanceDB insert).
    /// This is the transactional safety net — only marks AFTER the vector write succeeds.
    pub fn mark_promoted(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute("UPDATE dreams SET promoted = 1 WHERE id = ?1", params![id])
            .map_err(|e| format!("Mark promoted failed: {}", e))?;
        Ok(())
    }

    /// Count unpromoted dreams (for status display).
    pub fn unpromoted_count(&self) -> Result<u32, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM dreams WHERE promoted = 0", [], |r| {
                r.get(0)
            })
            .map_err(|e| format!("Count failed: {}", e))?;
        Ok(count as u32)
    }

    /// Clean up promoted dreams older than 7 days (housekeeping).
    pub fn cleanup_promoted(&self) -> Result<u32, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let cutoff = chrono::Utc::now().timestamp() - (7 * 86400);
        let deleted = conn
            .execute(
                "DELETE FROM dreams WHERE promoted = 1 AND created_at < ?1",
                params![cutoff],
            )
            .map_err(|e| format!("Cleanup failed: {}", e))?;
        Ok(deleted as u32)
    }
}
