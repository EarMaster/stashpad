use crate::{Context, StashItem, ContextRule}; 
use rusqlite::{params, Connection, Result, OptionalExtension};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DbManager {
    pub conn: Connection,
}

impl DbManager {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        
        // Enable WAL mode for better concurrency and performance
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;

        let manager = Self { conn };
        manager.init_tables()?;
        Ok(manager)
    }

    pub fn prepare_shutdown(&self) -> Result<()> {
        // Checkpoint WAL and truncate to clean up -wal and -shm files
        self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    fn init_tables(&self) -> Result<()> {
        // Contexts table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS contexts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                rules TEXT NOT NULL,
                last_used TEXT,
                updated_at INTEGER, 
                deleted BOOLEAN DEFAULT 0
            )",
            [],
        )?;

        // Stashes table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS stashes (
                id TEXT PRIMARY KEY,
                context_id TEXT,
                content TEXT NOT NULL,
                files TEXT NOT NULL,
                created_at TEXT NOT NULL,
                completed BOOLEAN DEFAULT 0,
                completed_at TEXT,
                position REAL,
                updated_at INTEGER,
                deleted BOOLEAN DEFAULT 0
            )",
            [],
        )?;

        self.conn.execute("CREATE INDEX IF NOT EXISTS idx_stashes_context ON stashes(context_id)", [])?;
        self.conn.execute("CREATE INDEX IF NOT EXISTS idx_stashes_position ON stashes(position)", [])?;

        Ok(())
    }

    pub fn migrate_from_json(&mut self, stashes: Vec<crate::StashItem>, contexts: Vec<crate::Context>) -> Result<()> {
        let tx = self.conn.transaction()?;

        // Contexts
        for ctx in contexts {
            let rules_json = serde_json::to_string(&ctx.rules).unwrap_or_default();
            tx.execute(
                "INSERT OR IGNORE INTO contexts (id, name, rules, last_used, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    ctx.id,
                    ctx.name,
                    rules_json,
                    ctx.last_used,
                    now_ts()
                ],
            )?;
        }

        // Stashes
        for (i, stash) in stashes.iter().enumerate() {
            let files_json = serde_json::to_string(&stash.files).unwrap_or_default();
            // Assign position based on index (assuming json list was ordered)
            let position = i as f64;
            tx.execute(
                "INSERT OR IGNORE INTO stashes (id, context_id, content, files, created_at, completed, completed_at, position, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    stash.id,
                    stash.context_id,
                    stash.content,
                    files_json,
                    stash.created_at,
                    stash.completed,
                    stash.completed_at,
                    position,
                    now_ts()
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    // --- Context CRUD ---

    pub fn get_contexts(&self) -> Result<Vec<Context>> {
        let mut stmt = self.conn.prepare("SELECT id, name, rules, last_used FROM contexts WHERE deleted = 0")?;
        let rows = stmt.query_map([], |row| {
            let rules_str: String = row.get(2)?;
            let rules: Vec<ContextRule> = serde_json::from_str(&rules_str).unwrap_or_default();
            Ok(Context {
                id: row.get(0)?,
                name: row.get(1)?,
                rules,
                last_used: row.get(3)?,
            })
        })?;

        let mut contexts = Vec::new();
        for context in rows {
            contexts.push(context?);
        }
        Ok(contexts)
    }

    pub fn save_context(&mut self, ctx: &Context) -> Result<()> {
        let rules_json = serde_json::to_string(&ctx.rules).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![
                ctx.id,
                ctx.name,
                rules_json,
                ctx.last_used,
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn delete_context(&mut self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE contexts SET deleted = 1, updated_at = ?2 WHERE id = ?1",
            params![id, now_ts()],
        )?;
        Ok(())
    }

    // --- Stash CRUD ---

    pub fn get_stashes(&self) -> Result<Vec<StashItem>> {
        // Order by position primarily
        let mut stmt = self.conn.prepare("SELECT id, context_id, content, files, created_at, completed, completed_at, position FROM stashes WHERE deleted = 0 ORDER BY position ASC")?;
        let rows = stmt.query_map([], |row| {
            let files_str: String = row.get(3)?;
            let files: Vec<String> = serde_json::from_str(&files_str).unwrap_or_default();
            Ok(StashItem {
                id: row.get(0)?,
                context_id: row.get(1)?,
                content: row.get(2)?,
                files,
                created_at: row.get(4)?,
                completed: row.get(5)?,
                completed_at: row.get(6)?,
            })
        })?;

        let mut stashes = Vec::new();
        for stash in rows {
            stashes.push(stash?);
        }
        Ok(stashes)
    }

    pub fn save_stash(&mut self, stash: &StashItem, position: Option<f64>) -> Result<()> {
        let files_json = serde_json::to_string(&stash.files).unwrap_or_default();
        
        // If position is NOT provided, we need to check if it's an update or insert
        // If insert, default to end (max position + 1)
        // If update, keep existing position
        
        let final_pos = if let Some(p) = position {
            p
        } else {
            // Check existing
            let existing_pos: Option<f64> = self.conn.query_row(
                "SELECT position FROM stashes WHERE id = ?1",
                params![stash.id],
                |row| row.get(0)
            ).optional()?;
            
            if let Some(p) = existing_pos {
                p
            } else {
                // New item, append to end
                let max_pos: Option<f64> = self.conn.query_row(
                    "SELECT MAX(position) FROM stashes WHERE deleted = 0",
                    [],
                    |row| row.get(0)
                ).optional()?;
                max_pos.unwrap_or(0.0) + 1.0
            }
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO stashes (id, context_id, content, files, created_at, completed, completed_at, position, updated_at, deleted) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![
                stash.id,
                stash.context_id,
                stash.content,
                files_json,
                stash.created_at,
                stash.completed,
                stash.completed_at,
                final_pos,
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn delete_stash(&mut self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE stashes SET deleted = 1, updated_at = ?2 WHERE id = ?1",
            params![id, now_ts()],
        )?;
        Ok(())
    }

    /// Update positions for a list of stashes. 
    /// Assuming the input list represents the new order.
    pub fn update_stash_positions(&mut self, stashes: &Vec<StashItem>) -> Result<()> {
         let tx = self.conn.transaction()?;
         for (i, stash) in stashes.iter().enumerate() {
             let pos = i as f64;
             tx.execute(
                 "UPDATE stashes SET position = ?2, updated_at = ?3 WHERE id = ?1",
                 params![stash.id, pos, now_ts()]
             )?;
         }
         tx.commit()?;
         Ok(())
    }
    
    pub fn delete_completed_stashes(&mut self, context_id: Option<String>) -> Result<()> {
        if let Some(ctx_id) = context_id {
             self.conn.execute(
                "UPDATE stashes SET deleted = 1, updated_at = ?2 WHERE completed = 1 AND context_id = ?1",
                params![ctx_id, now_ts()],
            )?;
        } else {
             self.conn.execute(
                "UPDATE stashes SET deleted = 1, updated_at = ?1 WHERE completed = 1",
                params![now_ts()],
            )?;
        }
        Ok(())
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
