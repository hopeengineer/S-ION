use lancedb::connect;
use lancedb::connection::Connection;
use lancedb::query::{ExecutableQuery, QueryBase};
use arrow_array::{
    RecordBatch, RecordBatchIterator, StringArray, Float32Array, Int32Array,
    Int64Array, BooleanArray, FixedSizeListArray, ArrayRef, Array,
};
use arrow_schema::{Schema, Field, DataType};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use futures_util::StreamExt;

// ──────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────

/// Memory categories with different lifecycle rules.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub enum MemoryCategory {
    Preference,  // Permanent, global
    Fact,        // Permanent, global
    Decision,    // Permanent, project
    Observation, // 30-day TTL, project
}

impl MemoryCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Preference => "preference",
            Self::Fact => "fact",
            Self::Decision => "decision",
            Self::Observation => "observation",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "preference" => Self::Preference,
            "fact" => Self::Fact,
            "decision" => Self::Decision,
            _ => Self::Observation,
        }
    }

    pub fn ttl_days(&self) -> Option<i32> {
        match self {
            Self::Observation => Some(30),
            _ => None,
        }
    }

    pub fn default_global(&self) -> bool {
        matches!(self, Self::Preference | Self::Fact)
    }
}

/// A stored memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub category: String,
    pub is_global: bool,
    pub created_at: i64,
    pub ttl_days: Option<i32>,
    pub metadata: String,
}

/// Search result from the memory system.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MemorySearchResult {
    pub entry: MemoryEntry,
    pub score: f32,
    pub source: String,
}

// ──────────────────────────────────────────────────
// LanceDB Schema
// ──────────────────────────────────────────────────

const VECTOR_DIM: i32 = 1024;
const TABLE_NAME: &str = "sion_memories";

fn memory_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("category", DataType::Utf8, false),
        Field::new("is_global", DataType::Boolean, false),
        Field::new(
            "local_vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                VECTOR_DIM,
            ),
            false,
        ),
        Field::new("created_at", DataType::Int64, false),
        Field::new("ttl_days", DataType::Int32, true),
        Field::new("metadata", DataType::Utf8, false),
    ]))
}

// ──────────────────────────────────────────────────
// MemoryManager
// ──────────────────────────────────────────────────

pub struct MemoryManager {
    global_db: Connection,
    project_db: Option<Connection>,
}

impl MemoryManager {
    pub async fn init(project_shadow_dir: Option<&Path>) -> Result<Self, String> {
        let data_dir = dirs::data_local_dir()
            .ok_or("Cannot determine OS data directory")?;
        let global_path = data_dir.join("com.s-ion.dev").join("memory.lance");
        std::fs::create_dir_all(&global_path)
            .map_err(|e| format!("Failed to create global memory dir: {}", e))?;

        let global_db = connect(global_path.to_str().unwrap())
            .execute()
            .await
            .map_err(|e| format!("Failed to connect global LanceDB: {}", e))?;

        Self::ensure_table(&global_db).await?;

        let project_db = if let Some(shadow) = project_shadow_dir {
            let lance_path = shadow.join("lance");
            std::fs::create_dir_all(&lance_path)
                .map_err(|e| format!("Failed to create project memory dir: {}", e))?;
            let db = connect(lance_path.to_str().unwrap())
                .execute()
                .await
                .map_err(|e| format!("Failed to connect project LanceDB: {}", e))?;
            Self::ensure_table(&db).await?;
            println!("🧠 Memory Manager initialized (Global + Project: {})", lance_path.display());
            Some(db)
        } else {
            println!("🧠 Memory Manager initialized (Global only: {})", global_path.display());
            None
        };

        Ok(Self { global_db, project_db })
    }

    async fn ensure_table(db: &Connection) -> Result<(), String> {
        let tables = db.table_names()
            .execute()
            .await
            .map_err(|e| format!("Failed to list tables: {}", e))?;

        if !tables.contains(&TABLE_NAME.to_string()) {
            let schema = memory_schema();
            let batch = RecordBatch::new_empty(schema.clone());
            let batches: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));
            db.create_table(TABLE_NAME, batches)
                .execute()
                .await
                .map_err(|e| format!("Failed to create table: {}", e))?;
        }
        Ok(())
    }

    /// Store a memory with cosine dedup (distance < 0.4 ≈ cosine > 0.92 → skip).
    pub async fn store(
        &self,
        content: &str,
        category: MemoryCategory,
        is_global: bool,
        vector: Vec<f32>,
        metadata: &str,
    ) -> Result<String, String> {
        let db = if is_global { &self.global_db } else {
            self.project_db.as_ref().unwrap_or(&self.global_db)
        };

        let table = db.open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| format!("Failed to open table: {}", e))?;

        // Dedup check
        let dedup_result = table.vector_search(vector.clone())
            .map_err(|e| format!("Dedup search failed: {}", e))?
            .limit(1)
            .execute()
            .await;

        if let Ok(mut stream) = dedup_result {
            while let Some(Ok(batch)) = stream.next().await {
                if batch.num_rows() > 0 {
                    if let Some(dist_col) = batch.column_by_name("_distance") {
                        if let Some(distances) = dist_col.as_any().downcast_ref::<Float32Array>() {
                            if distances.len() > 0 && distances.value(0) < 0.4 {
                                println!("🧠 Dedup: too similar (dist={}), skipping", distances.value(0));
                                return Ok("dedup_skipped".into());
                            }
                        }
                    }
                }
            }
        }

        // Insert
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let ttl = category.ttl_days();
        let schema = memory_schema();

        let id_arr = Arc::new(StringArray::from(vec![id.as_str()])) as ArrayRef;
        let content_arr = Arc::new(StringArray::from(vec![content])) as ArrayRef;
        let cat_arr = Arc::new(StringArray::from(vec![category.as_str()])) as ArrayRef;
        let global_arr = Arc::new(BooleanArray::from(vec![is_global])) as ArrayRef;

        let values = Arc::new(Float32Array::from(vector)) as ArrayRef;
        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_arr = Arc::new(FixedSizeListArray::new(field, VECTOR_DIM, values, None)) as ArrayRef;

        let time_arr = Arc::new(Int64Array::from(vec![now])) as ArrayRef;
        let ttl_arr = Arc::new(Int32Array::from(vec![ttl])) as ArrayRef;
        let meta_arr = Arc::new(StringArray::from(vec![metadata])) as ArrayRef;

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![id_arr, content_arr, cat_arr, global_arr, vector_arr, time_arr, ttl_arr, meta_arr],
        ).map_err(|e| format!("Batch creation failed: {}", e))?;

        let batches: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));
        table.add(batches)
            .execute()
            .await
            .map_err(|e| format!("Insert failed: {}", e))?;

        println!("🧠 Stored: [{}] {} ({})", category.as_str(),
            &content[..content.len().min(60)],
            if is_global { "global" } else { "project" });
        Ok(id)
    }

    /// Federated search across both tiers.
    pub async fn search(&self, query_vector: Vec<f32>, limit: usize) -> Result<Vec<MemorySearchResult>, String> {
        let mut all = Vec::new();

        if let Ok(r) = self.search_single(&self.global_db, &query_vector, limit, "global").await {
            all.extend(r);
        }
        if let Some(ref pdb) = self.project_db {
            if let Ok(r) = self.search_single(pdb, &query_vector, limit, "project").await {
                all.extend(r);
            }
        }

        all.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(limit);
        Ok(all)
    }

    async fn search_single(&self, db: &Connection, query: &[f32], limit: usize, source: &str) -> Result<Vec<MemorySearchResult>, String> {
        let table = db.open_table(TABLE_NAME).execute().await
            .map_err(|e| format!("Open table failed: {}", e))?;

        let mut stream = table.vector_search(query.to_vec())
            .map_err(|e| format!("Search failed: {}", e))?
            .limit(limit)
            .execute()
            .await
            .map_err(|e| format!("Search execute failed: {}", e))?;

        let mut results = Vec::new();

        while let Some(Ok(batch)) = stream.next().await {
            let n = batch.num_rows();
            for i in 0..n {
                let id = batch.column_by_name("id")
                    .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                    .map(|a| a.value(i).to_string()).unwrap_or_default();
                let content = batch.column_by_name("content")
                    .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                    .map(|a| a.value(i).to_string()).unwrap_or_default();
                let category = batch.column_by_name("category")
                    .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                    .map(|a| a.value(i).to_string()).unwrap_or_default();
                let is_global = batch.column_by_name("is_global")
                    .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
                    .map(|a| a.value(i)).unwrap_or(false);
                let created_at = batch.column_by_name("created_at")
                    .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
                    .map(|a| a.value(i)).unwrap_or(0);
                let ttl_days = batch.column_by_name("ttl_days")
                    .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
                    .map(|a| if a.is_null(i) { None } else { Some(a.value(i)) })
                    .unwrap_or(None);
                let metadata = batch.column_by_name("metadata")
                    .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                    .map(|a| a.value(i).to_string()).unwrap_or_default();
                let score = batch.column_by_name("_distance")
                    .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                    .map(|a| a.value(i)).unwrap_or(f32::MAX);

                results.push(MemorySearchResult {
                    entry: MemoryEntry { id, content, category, is_global, created_at, ttl_days, metadata },
                    score,
                    source: source.to_string(),
                });
            }
        }

        Ok(results)
    }

    /// Prune expired memories.
    pub async fn prune_expired(&self) -> Result<u32, String> {
        let now = chrono::Utc::now().timestamp();
        let filter = format!(
            "ttl_days IS NOT NULL AND created_at + CAST(ttl_days AS BIGINT) * 86400 < {}",
            now
        );

        for (db, label) in std::iter::once((&self.global_db, "global"))
            .chain(self.project_db.as_ref().map(|db| (db, "project")))
        {
            if let Ok(table) = db.open_table(TABLE_NAME).execute().await {
                match table.delete(&filter).await {
                    Ok(_) => println!("🧹 Pruned expired memories from {}", label),
                    Err(e) => println!("⚠️ Prune failed for {}: {}", label, e),
                }
            }
        }

        Ok(0)
    }
}
