use crate::{Context, StashItem, Attachment, ContextRule}; 
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

        // Attachments table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS attachments (
                id TEXT PRIMARY KEY,
                stash_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                mime_type TEXT,
                syntax TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(stash_id) REFERENCES stashes(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Check/Migrate syntax column
        let syntax_exists: bool = self.conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('attachments') WHERE name='syntax'",
                [],
                |row| row.get(0).map(|c: i32| c > 0),
            )
            .unwrap_or(false);

        if !syntax_exists {
            let _ = self.conn.execute("ALTER TABLE attachments ADD COLUMN syntax TEXT", []);
        }
        self.conn.execute("CREATE INDEX IF NOT EXISTS idx_attachments_stash_id ON attachments(stash_id)", [])?;

        // Migrate potentially existing files to attachments
        self.migrate_v1_files_to_attachments()?;

        // Ensure default context exists
        self.ensure_default_context()?;

        Ok(())
    }

    fn ensure_default_context(&self) -> Result<()> {
        // Check if default context exists
        let exists: bool = self.conn
            .query_row(
                "SELECT COUNT(*) FROM contexts WHERE id = 'default' AND deleted = 0",
                [],
                |row| row.get(0).map(|c: i32| c > 0),
            )
            .unwrap_or(false);

        if !exists {
            let now = chrono::Utc::now().to_rfc3339();
            
            // Create default context with empty rules
            self.conn.execute(
                "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted) VALUES ('default', 'Default', '[]', ?1, ?2, 0)",
                params![now, now_ts()],
            )?;

            // Create starter stashes to help new users
            // Completed stash: "Install Stashpad and start it"
            let completed_stash_id = uuid::Uuid::new_v4().to_string();
            self.conn.execute(
                "INSERT INTO stashes (id, context_id, content, files, created_at, completed, completed_at, position, updated_at, deleted) VALUES (?1, 'default', ?2, '[]', ?3, 1, ?3, 1.0, ?4, 0)",
                params![
                    completed_stash_id,
                    "Install Stashpad and start it ✓",
                    now,
                    now_ts()
                ],
            )?;

            // Active stash: "Create a context for your project"
            let active_stash_id = uuid::Uuid::new_v4().to_string();
            self.conn.execute(
                "INSERT INTO stashes (id, context_id, content, files, created_at, completed, completed_at, position, updated_at, deleted) VALUES (?1, 'default', ?2, '[]', ?3, 0, NULL, 2.0, ?4, 0)",
                params![
                    active_stash_id,
                    "Create a context for your project",
                    now,
                    now_ts()
                ],
            )?;
        }

        Ok(())
    }

    fn migrate_v1_files_to_attachments(&self) -> Result<()> {
        // Query stashes with files
        let mut stmt = self.conn.prepare("SELECT id, files, created_at FROM stashes WHERE files != '[]' AND files != ''")?;
        
        let rows = stmt.query_map([], |row| {
             let id: String = row.get(0)?;
             let files_str: String = row.get(1)?;
             let created_at: String = row.get(2)?;
             Ok((id, files_str, created_at))
        })?;

        let mut stashes_to_migrate = Vec::new();
        for r in rows {
            if let Ok(val) = r {
                stashes_to_migrate.push(val);
            }
        }

        if stashes_to_migrate.is_empty() {
            return Ok(());
        }

        println!("Migrating v1 files to attachments for {} stashes...", stashes_to_migrate.len());
        
        // Transaction for migration
        // We need to use execute directly on self inside mutable methods, but here we only have &self.
        // However, sqlite connection is interior mutable in wrapping structs... check signature.
        // Connection is not interior mutable, but DbManager holds it. 
        // Logic check: init_tables is &self, passing self.conn. 
        // We can execute on self.conn.

        for (stash_id, files_str, created_at) in stashes_to_migrate {
             let files: Vec<String> = serde_json::from_str(&files_str).unwrap_or_default();
             
             for file_path in files {
                 let path = Path::new(&file_path);
                 if !path.exists() {
                     continue; // Skip non-existent, or maybe we should keep record? Let's skip invalid.
                 }
                 
                 let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                 let metadata = std::fs::metadata(&path);
                 let file_size = metadata.map(|m| m.len()).unwrap_or(0) as i64;
                 
                 // Generate ID (simple UUID v4 like)
                 use uuid::Uuid;
                 let att_id = Uuid::new_v4().to_string();
                 
                 // Extension mime guess
                 let mime_type = mime_guess::from_path(&path).first().map(|m| m.to_string());

                 self.conn.execute(
                     "INSERT OR IGNORE INTO attachments (id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                     params![
                         att_id,
                         stash_id,
                         file_path,
                         file_name,
                         file_size,
                         mime_type,
                         None::<String>, // syntax
                         created_at // Use stash creation time as fallback
                     ]
                 )?;
             }
             
             // Clear files column to avoid re-migration
             self.conn.execute("UPDATE stashes SET files = '[]' WHERE id = ?1", params![stash_id])?;
        }

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
        // Protect default context from being renamed or having rules modified
        let (name, rules_json) = if ctx.id == "default" {
            // Force default context to keep its name and empty rules
            ("Default".to_string(), "[]".to_string())
        } else {
            (ctx.name.clone(), serde_json::to_string(&ctx.rules).unwrap_or_default())
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![
                ctx.id,
                name,
                rules_json,
                ctx.last_used,
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn delete_context(&mut self, id: &str) -> Result<()> {
        // Protect default context from being deleted
        if id == "default" {
            return Ok(()); // Silently ignore deletion attempts
        }

        self.conn.execute(
            "UPDATE contexts SET deleted = 1, updated_at = ?2 WHERE id = ?1",
            params![id, now_ts()],
        )?;
        Ok(())
    }

    // --- Stash CRUD ---

    pub fn get_stashes(&self) -> Result<Vec<StashItem>> {
        // 1. Get all stashes
        let mut stmt = self.conn.prepare("SELECT id, context_id, content, files, created_at, completed, completed_at, position FROM stashes WHERE deleted = 0 ORDER BY position ASC")?;
        
        let stash_rows = stmt.query_map([], |row| {
            let files_str: String = row.get(3)?;
            // files_str kept for backward compat or if needed, but we now use attachments table.
            // We'll populate attachments below.
            
            Ok(StashItem {
                id: row.get(0)?,
                context_id: row.get(1)?,
                content: row.get(2)?,
                files: serde_json::from_str(&files_str).unwrap_or_default(),
                attachments: Vec::new(), // Populate later
                created_at: row.get(4)?,
                completed: row.get(5)?,
                completed_at: row.get(6)?,
            })
        })?;

        let mut stashes = Vec::new();
        for s in stash_rows {
            stashes.push(s?);
        }

        // 2. Get all attachments (could be optimized with a JOIN, but separate query is cleaner for mapping)
        let mut att_stmt = self.conn.prepare("SELECT id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at FROM attachments")?;
        
        let att_rows = att_stmt.query_map([], |row| {
             Ok(Attachment {
                 id: row.get(0)?,
                 stash_id: row.get(1)?,
                 file_path: row.get(2)?,
                 file_name: row.get(3)?,
                 file_size: row.get(4)?,
                 mime_type: row.get(5)?,
                 syntax: row.get(6)?,
                 created_at: row.get(7)?,
             })
        })?;

        // Group by stash_id
        let mut attachments_map: std::collections::HashMap<String, Vec<Attachment>> = std::collections::HashMap::new();
        for att in att_rows {
            if let Ok(a) = att {
                attachments_map.entry(a.stash_id.clone()).or_default().push(a);
            }
        }

        // 3. Assign attachments to stashes
        for stash in &mut stashes {
            if let Some(atts) = attachments_map.remove(&stash.id) {
                stash.attachments = atts;
            }
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

        // UPSERT attachments (crucial for new stashes where save_asset might have failed FK)
        for att in &stash.attachments {
            self.conn.execute(
                "INSERT OR REPLACE INTO attachments (id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    att.id,
                    att.stash_id,
                    att.file_path,
                    att.file_name,
                    att.file_size,
                    att.mime_type,
                    att.syntax,
                    att.created_at
                ]
            )?;
        }

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
