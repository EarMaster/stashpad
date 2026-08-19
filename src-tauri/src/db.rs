use crate::models::{Context, StashItem, Attachment, ContextRule}; 
use rusqlite::{params, Connection, Result, OptionalExtension};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Which clock owns `updated_at` for a given write.
///
/// This distinction is the difference between working and broken cross-device sync.
/// Both paths used to share `updated_at.unwrap_or_else(now_ts)`, and because the UI
/// spreads the loaded stash back into the object it saves (`{ ...item, completed }`),
/// every local edit re-sent the timestamp the server had already stored. The server's
/// last-write-wins check is `client.updated_at > server.updated_at`, so the comparison
/// was `X > X` — false — and edits were silently discarded forever.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteOrigin {
    /// The user changed something on this device. Stamp the current time so the record
    /// is genuinely newer than the server's copy.
    LocalEdit,
    /// Applying data received from the server. Preserve the incoming timestamp verbatim
    /// so both sides keep comparing the same value and the merge converges.
    SyncImport,
}

impl WriteOrigin {
    /// Resolve the `updated_at` to persist for this write.
    fn stamp(self, supplied: Option<u64>) -> u64 {
        match self {
            WriteOrigin::LocalEdit => now_ts(),
            WriteOrigin::SyncImport => supplied.unwrap_or_else(now_ts),
        }
    }
}

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
                deleted BOOLEAN DEFAULT 0,
                description TEXT
            )",
            [],
        )?;

        // Check/Migrate description column for contexts
        let description_exists: bool = self.conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('contexts') WHERE name='description'",
                [],
                |row| row.get(0).map(|c: i32| c > 0),
            )
            .unwrap_or(false);

        if !description_exists {
            let _ = self.conn.execute("ALTER TABLE contexts ADD COLUMN description TEXT", []);
        }

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

        // Tracks when this attachment's bytes were successfully pushed to the cloud.
        // Without it every sync re-uploaded every file that ever existed, because the
        // upload path had no idempotency check at all.
        let uploaded_at_exists: bool = self.conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('attachments') WHERE name='uploaded_at'",
                [],
                |row| row.get(0).map(|c: i32| c > 0),
            )
            .unwrap_or(false);

        if !uploaded_at_exists {
            let _ = self.conn.execute("ALTER TABLE attachments ADD COLUMN uploaded_at INTEGER", []);
        }

        // Marks a record as having local changes the server has not acknowledged yet, so
        // sync can push only what changed instead of the entire table every time.
        //
        // Deliberately a flag rather than a timestamp comparison: `updated_at` is the
        // client's clock while the sync cursor is the server's, and comparing the two is
        // exactly the mistake that made records invisible to sync before.
        //
        // Existing rows default to 1, so the first sync after upgrading pushes
        // everything once and the server ends up with a complete picture.
        for table in ["stashes", "contexts"] {
            let has_flag: bool = self
                .conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='pending_sync'",
                        table
                    ),
                    [],
                    |row| row.get(0).map(|c: i32| c > 0),
                )
                .unwrap_or(false);

            if !has_flag {
                let _ = self.conn.execute(
                    &format!(
                        "ALTER TABLE {} ADD COLUMN pending_sync INTEGER NOT NULL DEFAULT 1",
                        table
                    ),
                    [],
                );
            }

            let _ = self.conn.execute(
                &format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_pending ON {}(pending_sync)",
                    table, table
                ),
                [],
            );
        }

        self.conn.execute("CREATE INDEX IF NOT EXISTS idx_attachments_stash_id ON attachments(stash_id)", [])?;

        // Migrate enhanced_content column for AI enhancement feature
        let enhanced_content_exists: bool = self.conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('stashes') WHERE name='enhanced_content'",
                [],
                |row| row.get(0).map(|c: i32| c > 0),
            )
            .unwrap_or(false);

        if !enhanced_content_exists {
            let _ = self.conn.execute("ALTER TABLE stashes ADD COLUMN enhanced_content TEXT", []);
        }

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
        for (stash_id, files_str, created_at) in stashes_to_migrate {
             let files: Vec<String> = serde_json::from_str(&files_str).unwrap_or_default();
             
             for file_path in files {
                 let path = Path::new(&file_path);
                 if !path.exists() {
                     continue; // Skip non-existent
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

    pub fn migrate_from_json(&mut self, stashes: Vec<StashItem>, contexts: Vec<Context>) -> Result<()> {
        let tx = self.conn.transaction()?;

        // Contexts
        for ctx in contexts {
            let rules_json = serde_json::to_string(&ctx.rules).unwrap_or_default();
            tx.execute(
                "INSERT OR IGNORE INTO contexts (id, name, rules, last_used, updated_at, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    ctx.id,
                    ctx.name,
                    rules_json,
                    ctx.last_used,
                    now_ts(),
                    ctx.description
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
        let mut stmt = self.conn.prepare("SELECT id, name, rules, last_used, updated_at, description FROM contexts WHERE deleted = 0")?;
        let rows = stmt.query_map([], |row| {
            let rules_str: String = row.get(2)?;
            let rules: Vec<ContextRule> = serde_json::from_str(&rules_str).unwrap_or_default();
            Ok(Context {
                id: row.get(0)?,
                name: row.get(1)?,
                rules,
                last_used: row.get(3)?,
                updated_at: row.get(4)?,
                deleted: false,
                description: row.get(5)?,
            })
        })?;

        let mut contexts = Vec::new();
        for context in rows {
            contexts.push(context?);
        }
        Ok(contexts)
    }

    pub fn save_context(&mut self, ctx: &Context, origin: WriteOrigin) -> Result<()> {
        // Protect default context from being renamed or having rules modified
        let (name, rules_json) = if ctx.id == "default" {
            // Force default context to keep its name and empty rules
            ("Default".to_string(), "[]".to_string())
        } else {
            (ctx.name.clone(), serde_json::to_string(&ctx.rules).unwrap_or_default())
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted, description, pending_sync) VALUES (?1, ?2, ?3, ?4, ?5, ?7, ?6, ?8)",
            params![
                ctx.id,
                name,
                rules_json,
                ctx.last_used,
                origin.stamp(ctx.updated_at),
                ctx.description,
                if ctx.deleted { 1 } else { 0 },
                // Local edits still need pushing; server data is already in sync.
                if origin == WriteOrigin::LocalEdit { 1 } else { 0 }
            ],
        )?;
        Ok(())
    }

    /// Apply contexts received from the server in one transaction, preserving their
    /// timestamps so last-write-wins stays stable across devices.
    pub fn import_contexts(&mut self, contexts: &[Context]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for ctx in contexts {
            // The default context keeps its name and rules on every device.
            let (name, rules_json) = if ctx.id == "default" {
                ("Default".to_string(), "[]".to_string())
            } else {
                (
                    ctx.name.clone(),
                    serde_json::to_string(&ctx.rules).unwrap_or_default(),
                )
            };

            tx.execute(
                "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted, description, pending_sync) VALUES (?1, ?2, ?3, ?4, ?5, ?7, ?6, 0)",
                params![
                    ctx.id,
                    name,
                    rules_json,
                    ctx.last_used,
                    WriteOrigin::SyncImport.stamp(ctx.updated_at),
                    ctx.description,
                    if ctx.deleted { 1 } else { 0 }
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_context(&mut self, id: &str) -> Result<()> {
        // Protect default context from being deleted
        if id == "default" {
            return Ok(()); // Silently ignore deletion attempts
        }

        self.conn.execute(
            "UPDATE contexts SET deleted = 1, updated_at = ?2, pending_sync = 1 WHERE id = ?1",
            params![id, now_ts()],
        )?;
        Ok(())
    }

    // --- Stash CRUD ---

    pub fn get_stashes(&self) -> Result<Vec<StashItem>> {
        // 1. Get all stashes
        let mut stmt = self.conn.prepare("SELECT id, context_id, content, files, created_at, completed, completed_at, position, updated_at, enhanced_content FROM stashes WHERE deleted = 0 ORDER BY position ASC")?;
        
        let stash_rows = stmt.query_map([], |row| {
            let files_str: String = row.get(3)?;
            // files_str kept for backward compat or if needed, but we now use attachments table.
            // We'll populate attachments below.
            
            Ok(StashItem {
                id: row.get(0)?,
                context_id: row.get(1)?,
                content: row.get(2)?,
                enhanced_content: row.get(9)?,
                files: serde_json::from_str(&files_str).unwrap_or_default(),
                attachments: Vec::new(), // Populate later
                created_at: row.get(4)?,
                completed: row.get(5)?,
                completed_at: row.get(6)?,
                updated_at: row.get(8)?,
                deleted: false,
            })
        })?;

        let mut stashes = Vec::new();
        for s in stash_rows {
            stashes.push(s?);
        }

        // 2. Get all attachments
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

    pub fn get_stashes_for_sync(&mut self) -> Result<Vec<StashItem>> {
        self.load_stashes_for_sync_where("")
    }

    /// Shared loader for the sync payload. `filter` is a trusted, hard-coded SQL
    /// fragment - never caller input.
    fn load_stashes_for_sync_where(&mut self, filter: &str) -> Result<Vec<StashItem>> {
        let sql = format!("SELECT id, context_id, content, files, created_at, completed, completed_at, position, updated_at, enhanced_content, deleted FROM stashes {}", filter);
        let mut stmt = self.conn.prepare(&sql)?;
        
        let stash_rows = stmt.query_map([], |row| {
            let files_str: String = row.get(3)?;
            let deleted_int: i32 = row.get(10)?;
            
            Ok(StashItem {
                id: row.get(0)?,
                context_id: row.get(1)?,
                content: row.get(2)?,
                enhanced_content: row.get(9)?,
                files: serde_json::from_str(&files_str).unwrap_or_default(),
                attachments: Vec::new(),
                created_at: row.get(4)?,
                completed: row.get(5)?,
                completed_at: row.get(6)?,
                updated_at: row.get(8)?,
                deleted: deleted_int != 0,
            })
        })?;

        let mut stashes = Vec::new();
        for s in stash_rows {
            stashes.push(s?);
        }

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

        let mut attachments_map: std::collections::HashMap<String, Vec<Attachment>> = std::collections::HashMap::new();
        for att in att_rows {
            if let Ok(a) = att {
                attachments_map.entry(a.stash_id.clone()).or_default().push(a);
            }
        }

        for stash in &mut stashes {
            if let Some(atts) = attachments_map.remove(&stash.id) {
                stash.attachments = atts;
            }
        }

        Ok(stashes)
    }

    /// Only these two tables carry the sync flag. Guards against interpolating an
    /// arbitrary caller-supplied name into SQL.
    fn is_syncable_table(table: &str) -> bool {
        table == "stashes" || table == "contexts"
    }

    /// Take ownership of everything waiting to be pushed, marking it in flight.
    ///
    /// `pending_sync` is a small state machine rather than a boolean:
    ///   0 = in sync, 1 = changed locally, 2 = included in a push that is in flight.
    ///
    /// Claiming moves 1 → 2 and returns those ids. A local edit during the request writes
    /// 1 again, so the acknowledgement - which only clears rows still at 2 - leaves it
    /// queued and it goes out next cycle. A push that fails or crashes leaves rows at 2,
    /// which still counts as pending, so they are simply re-claimed later.
    ///
    /// This replaced comparing `updated_at` before and after the push: that column has
    /// one-second granularity, so an edit landing in the same second as the version being
    /// sent was indistinguishable from no edit at all, and would have been dropped.
    fn claim_pending(&mut self, table: &str) -> Result<std::collections::HashSet<String>> {
        if !Self::is_syncable_table(table) {
            return Ok(std::collections::HashSet::new());
        }

        let mut ids = std::collections::HashSet::new();
        {
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT id FROM {} WHERE pending_sync > 0", table))?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for id in rows {
                ids.insert(id?);
            }
        }

        self.conn.execute(
            &format!(
                "UPDATE {} SET pending_sync = 2 WHERE pending_sync > 0",
                table
            ),
            [],
        )?;

        Ok(ids)
    }

    /// Stashes with local changes the server has not acknowledged, marked in flight.
    pub fn claim_pending_stashes(&mut self) -> Result<Vec<StashItem>> {
        // Claiming moves every pending row to the in-flight state, so selecting on it
        // returns exactly what was claimed. Filtering in SQL rather than loading the
        // whole table and discarding most of it: with a few hundred stashes that read
        // is the bulk of a sync's local cost, and it runs on every keystroke-triggered
        // sync.
        self.claim_pending("stashes")?;
        self.load_stashes_for_sync_where("WHERE pending_sync = 2")
    }

    /// Contexts with local changes the server has not acknowledged, marked in flight.
    pub fn claim_pending_contexts(&mut self) -> Result<Vec<Context>> {
        // Same reasoning as claim_pending_stashes: select the claimed rows rather than
        // loading every context and discarding most of them.
        self.claim_pending("contexts")?;
        self.load_contexts_for_sync_where("WHERE pending_sync = 2")
    }

    /// Mark records the server accepted as synced.
    ///
    /// Only clears rows still in the in-flight state, so anything edited while the push
    /// was running stays queued.
    pub fn mark_synced(&mut self, table: &str, ids: &[String]) -> Result<()> {
        if ids.is_empty() || !Self::is_syncable_table(table) {
            return Ok(());
        }

        let sql = format!(
            "UPDATE {} SET pending_sync = 0 WHERE id = ?1 AND pending_sync = 2",
            table
        );
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute(&sql, params![id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_contexts_for_sync(&mut self) -> Result<Vec<Context>> {
        self.load_contexts_for_sync_where("")
    }

    /// Shared loader for the context sync payload. `filter` is a trusted, hard-coded SQL
    /// fragment - never caller input.
    fn load_contexts_for_sync_where(&mut self, filter: &str) -> Result<Vec<Context>> {
        let sql = format!("SELECT id, name, rules, last_used, updated_at, deleted, description FROM contexts {}", filter);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            let rules_json: String = row.get(2)?;
            let deleted_int: i32 = row.get(5)?;
            Ok(Context {
                id: row.get(0)?,
                name: row.get(1)?,
                rules: serde_json::from_str(&rules_json).unwrap_or_default(),
                last_used: row.get(3)?,
                updated_at: row.get(4)?,
                deleted: deleted_int != 0,
                description: row.get(6)?,
            })
        })?;

        let mut contexts = Vec::new();
        for r in rows {
            contexts.push(r?);
        }
        Ok(contexts)
    }

    pub fn import_stashes(&mut self, stashes: &Vec<StashItem>) -> Result<()> {
        let tx = self.conn.transaction()?;
        for stash in stashes {
            let files_json = serde_json::to_string(&stash.files).unwrap_or_default();
            
            let existing_pos: Option<f64> = tx.query_row(
                "SELECT position FROM stashes WHERE id = ?1",
                params![stash.id],
                |row| row.get(0)
            ).optional()?;
            
            let final_pos = if let Some(p) = existing_pos {
                p
            } else {
                let max_pos: Option<f64> = tx.query_row(
                    "SELECT MAX(position) FROM stashes WHERE deleted = 0",
                    [],
                    |row| row.get(0)
                ).optional()?;
                max_pos.unwrap_or(0.0) + 1.0
            };

            tx.execute(
                // Upsert, never INSERT OR REPLACE: REPLACE deletes the existing row first, and
                // attachments reference stashes ON DELETE CASCADE, so replacing a stash
                // silently destroys every attachment hanging off it. ON CONFLICT updates
                // the row in place and leaves the children alone.
                "INSERT INTO stashes (id, context_id, content, enhanced_content, files, created_at, completed, completed_at, position, updated_at, deleted, pending_sync) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0) ON CONFLICT(id) DO UPDATE SET context_id=excluded.context_id, content=excluded.content, enhanced_content=excluded.enhanced_content, files=excluded.files, created_at=excluded.created_at, completed=excluded.completed, completed_at=excluded.completed_at, position=excluded.position, updated_at=excluded.updated_at, deleted=excluded.deleted, pending_sync=0",
                params![
                    stash.id,
                    stash.context_id,
                    stash.content,
                    stash.enhanced_content,
                    files_json,
                    stash.created_at,
                    stash.completed,
                    stash.completed_at,
                    final_pos,
                    // Server-supplied value, preserved verbatim.
                    WriteOrigin::SyncImport.stamp(stash.updated_at),
                    if stash.deleted { 1 } else { 0 }
                ],
            )?;

            for att in &stash.attachments {
                tx.execute(
                    // Never blank an existing local path: the server has no concept of a
                    // local file_path and always sends it empty, so an incoming empty
                    // value means "unknown", not "cleared".
                    "INSERT INTO attachments (id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(id) DO UPDATE SET stash_id=excluded.stash_id, file_path=CASE WHEN TRIM(excluded.file_path)='' THEN attachments.file_path ELSE excluded.file_path END, file_name=excluded.file_name, file_size=excluded.file_size, mime_type=excluded.mime_type, syntax=excluded.syntax",
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
        }
        tx.commit()?;
        Ok(())
    }

    pub fn save_stash(
        &mut self,
        stash: &StashItem,
        position: Option<f64>,
        origin: WriteOrigin,
    ) -> Result<()> {
        let files_json = serde_json::to_string(&stash.files).unwrap_or_default();
        
        // If position is NOT provided, we need to check if it's an update or insert
        
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
            // Upsert, never INSERT OR REPLACE: REPLACE deletes the existing row first, and
                // attachments reference stashes ON DELETE CASCADE, so replacing a stash
                // silently destroys every attachment hanging off it. ON CONFLICT updates
                // the row in place and leaves the children alone.
                "INSERT INTO stashes (id, context_id, content, enhanced_content, files, created_at, completed, completed_at, position, updated_at, deleted, pending_sync) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) ON CONFLICT(id) DO UPDATE SET context_id=excluded.context_id, content=excluded.content, enhanced_content=excluded.enhanced_content, files=excluded.files, created_at=excluded.created_at, completed=excluded.completed, completed_at=excluded.completed_at, position=excluded.position, updated_at=excluded.updated_at, deleted=excluded.deleted, pending_sync=excluded.pending_sync",
            params![
                stash.id,
                stash.context_id,
                stash.content,
                stash.enhanced_content,
                files_json,
                stash.created_at,
                stash.completed,
                stash.completed_at,
                final_pos,
                origin.stamp(stash.updated_at),
                if stash.deleted { 1 } else { 0 },
                // A local edit still needs pushing; data that just came from the server
                // is already in sync by definition.
                if origin == WriteOrigin::LocalEdit { 1 } else { 0 }
            ],
        )?;

        // UPSERT attachments (crucial for new stashes where save_asset might have failed FK)
        for att in &stash.attachments {
            self.conn.execute(
                // Never blank an existing local path: the server has no concept of a
                    // local file_path and always sends it empty, so an incoming empty
                    // value means "unknown", not "cleared".
                    "INSERT INTO attachments (id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(id) DO UPDATE SET stash_id=excluded.stash_id, file_path=CASE WHEN TRIM(excluded.file_path)='' THEN attachments.file_path ELSE excluded.file_path END, file_name=excluded.file_name, file_size=excluded.file_size, mime_type=excluded.mime_type, syntax=excluded.syntax",
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
            "UPDATE stashes SET deleted = 1, updated_at = ?2, pending_sync = 1 WHERE id = ?1",
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
                 "UPDATE stashes SET position = ?2, updated_at = ?3, pending_sync = 1 WHERE id = ?1",
                 params![stash.id, pos, now_ts()]
             )?;
         }
         tx.commit()?;
         Ok(())
    }
    
    pub fn delete_completed_stashes(&mut self, context_id: Option<String>) -> Result<()> {
        if let Some(ctx_id) = context_id {
             self.conn.execute(
                "UPDATE stashes SET deleted = 1, updated_at = ?2, pending_sync = 1 WHERE completed = 1 AND context_id = ?1",
                params![ctx_id, now_ts()],
            )?;
        } else {
             self.conn.execute(
                "UPDATE stashes SET deleted = 1, updated_at = ?1, pending_sync = 1 WHERE completed = 1",
                params![now_ts()],
            )?;
        }
        Ok(())
    }
}

pub fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    
    /// Helper to create an in-memory test database
    fn create_test_db() -> DbManager {
        let conn = Connection::open_in_memory().expect("Failed to create in-memory database");
        let manager = DbManager { conn };
        manager.init_tables().expect("Failed to initialize tables");
        manager
    }
    
    #[test]
    fn test_default_context_creation() {
        let db = create_test_db();
        
        // Default context should be created automatically
        let contexts = db.get_contexts().expect("Failed to get contexts");
        assert!(contexts.len() >= 1, "Should have at least default context");
        
        let default_ctx = contexts.iter().find(|c| c.id == "default");
        assert!(default_ctx.is_some(), "Default context should exist");
        assert_eq!(default_ctx.unwrap().name, "Default");
    }
    
    #[test]
    fn test_save_and_get_context() {
        let mut db = create_test_db();
        
        let test_context = Context {
            id: "test-project".to_string(),
            name: "Test Project".to_string(),
            rules: vec![
                ContextRule {
                    rule_type: "process".to_string(),
                    value: "code".to_string(),
                    match_type: "exact".to_string(),
                    match_case: false,
                    use_regex: false,
                }
            ],
            last_used: Some(chrono::Utc::now().to_rfc3339()),
            description: Some("Test description".to_string()),
            updated_at: None,
            deleted: false,
        };
        
        db.save_context(&test_context, WriteOrigin::LocalEdit).expect("Failed to save context");
        
        let contexts = db.get_contexts().expect("Failed to get contexts");
        let saved = contexts.iter().find(|c| c.id == "test-project");
        
        assert!(saved.is_some(), "Context should be saved");
        assert_eq!(saved.unwrap().name, "Test Project");
        assert_eq!(saved.unwrap().description, Some("Test description".to_string()));
    }
    
    #[test]
    fn test_default_context_protection() {
        let mut db = create_test_db();
        
        // Try to rename default context
        let modified_default = Context {
            id: "default".to_string(),
            name: "Modified Name".to_string(),  // Should be ignored
            rules: vec![],
            last_used: None,
            description: None,
            updated_at: None,
            deleted: false,
        };
        
        db.save_context(&modified_default, WriteOrigin::LocalEdit).expect("Save should succeed");
        
        let contexts = db.get_contexts().expect("Failed to get contexts");
        let default = contexts.iter().find(|c| c.id == "default").unwrap();
        
        // Name should still be "Default", protected from modification
        assert_eq!(default.name, "Default", "Default context name should be protected");
    }
    
    #[test]
    fn test_save_and_get_stash() {
        let mut db = create_test_db();
        
        let stash = StashItem {
            id: "test-stash-1".to_string(),
            context_id: Some("default".to_string()),
            content: "Test stash content".to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: false,
            completed_at: None,
            updated_at: None,
            deleted: false,
        };
        
        db.save_stash(&stash, None, WriteOrigin::LocalEdit).expect("Failed to save stash");
        
        let stashes = db.get_stashes().expect("Failed to get stashes");
        let saved = stashes.iter().find(|s| s.id == "test-stash-1");
        
        assert!(saved.is_some(), "Stash should be saved");
        assert_eq!(saved.unwrap().content, "Test stash content");
        assert_eq!(saved.unwrap().completed, false);
    }
    
    #[test]
    fn test_delete_stash() {
        let mut db = create_test_db();
        
        let stash = StashItem {
            id: "stash-to-delete".to_string(),
            context_id: Some("default".to_string()),
            content: "Will be deleted".to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: false,
            completed_at: None,
            updated_at: None,
            deleted: false,
        };
        
        db.save_stash(&stash, None, WriteOrigin::LocalEdit).expect("Failed to save stash");
        db.delete_stash("stash-to-delete").expect("Failed to delete stash");
        
        let stashes = db.get_stashes().expect("Failed to get stashes");
        let deleted = stashes.iter().find(|s| s.id == "stash-to-delete");
        
        assert!(deleted.is_none(), "Stash should be soft-deleted");
    }
    
    #[test]
    fn test_delete_completed_stashes() {
        let mut db = create_test_db();
        
        // Create completed and active stashes
        let completed = StashItem {
            id: "completed-1".to_string(),
            context_id: Some("default".to_string()),
            content: "Completed task".to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: true,
            completed_at: Some(chrono::Utc::now().to_rfc3339()),
            updated_at: None,
            deleted: false,
        };
        
        let active = StashItem {
            id: "active-1".to_string(),
            context_id: Some("default".to_string()),
            content: "Active task".to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: false,
            completed_at: None,
            updated_at: None,
            deleted: false,
        };
        
        db.save_stash(&completed, None, WriteOrigin::LocalEdit).expect("Failed to save completed stash");
        db.save_stash(&active, None, WriteOrigin::LocalEdit).expect("Failed to save active stash");
        
        db.delete_completed_stashes(None).expect("Failed to delete completed stashes");
        
        let stashes = db.get_stashes().expect("Failed to get stashes");
        
        assert!(stashes.iter().find(|s| s.id == "completed-1").is_none(), "Completed stash should be deleted");
        assert!(stashes.iter().find(|s| s.id == "active-1").is_some(), "Active stash should remain");
    }
    
    #[test]
    fn test_stash_positioning() {
        let mut db = create_test_db();
        
        let stash1 = StashItem {
            id: "pos-1".to_string(),
            context_id: Some("default".to_string()),
            content: "First".to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: false,
            completed_at: None,
            updated_at: None,
            deleted: false,
        };
        
        let stash2 = StashItem {
            id: "pos-2".to_string(),
            context_id: Some("default".to_string()),
            content: "Second".to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: false,
            completed_at: None,
            updated_at: None,
            deleted: false,
        };
        
        // Save without explicit position (should append)
        db.save_stash(&stash1, None, WriteOrigin::LocalEdit).expect("Failed to save stash1");
        db.save_stash(&stash2, None, WriteOrigin::LocalEdit).expect("Failed to save stash2");
        
        let stashes = db.get_stashes().expect("Failed to get stashes");
        
        // Should be ordered by position
        let pos1_idx = stashes.iter().position(|s| s.id == "pos-1");
        let pos2_idx = stashes.iter().position(|s| s.id == "pos-2");
        
        assert!(pos1_idx.is_some() && pos2_idx.is_some(), "Both stashes should exist");
        assert!(pos1_idx.unwrap() < pos2_idx.unwrap(), "Stashes should be in insertion order");
    }

    #[test]
    fn test_migrate_v1_files_to_attachments() {
        let db = create_test_db();
        
        // Create a temporary file to test migration
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test_file.txt");
        std::fs::write(&file_path, "test file content").expect("Failed to write test file");
        
        // Manually insert a stash with "v1" style files
        let stash_id = "v1-stash".to_string();
        let files_json = format!("[\"{}\"]", file_path.to_string_lossy().replace("\\", "\\\\"));
        let now = chrono::Utc::now().to_rfc3339();
        
        db.conn.execute(
            "INSERT INTO stashes (id, context_id, content, files, created_at, completed, position, updated_at) VALUES (?1, 'default', 'v1 content', ?2, ?3, 0, 1.0, ?4)",
            params![stash_id, files_json, now, now_ts()],
        ).expect("Failed to insert v1 stash");
        
        // Verify it was inserted
        let files_check: String = db.conn.query_row("SELECT files FROM stashes WHERE id = 'v1-stash'", [], |row| row.get(0)).unwrap();
        assert_eq!(files_check, files_json);
        
        // Run migration
        db.migrate_v1_files_to_attachments().expect("Migration failed");
        
        // Check if files column is cleared
        let files_after: String = db.conn.query_row("SELECT files FROM stashes WHERE id = 'v1-stash'", [], |row| row.get(0)).unwrap();
        assert_eq!(files_after, "[]");
        
        // Check if attachments were created
        let count: i32 = db.conn.query_row("SELECT COUNT(*) FROM attachments WHERE stash_id = 'v1-stash'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1, "Should have 1 attachment after migration");
    }

    /// Build a stash carrying an explicit `updated_at`, as the UI does when it spreads
    /// a loaded stash back into the object it saves.
    fn stash_with_updated_at(id: &str, content: &str, updated_at: Option<u64>) -> StashItem {
        StashItem {
            id: id.to_string(),
            context_id: Some("default".to_string()),
            content: content.to_string(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            completed: false,
            completed_at: None,
            updated_at,
            deleted: false,
        }
    }

    fn stored_updated_at(db: &DbManager, id: &str) -> u64 {
        db.conn
            .query_row(
                "SELECT updated_at FROM stashes WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .expect("stash should exist")
    }

    #[test]
    fn local_edit_advances_updated_at_even_when_the_caller_supplies_one() {
        // The regression that broke cross-device sync: the UI spreads the loaded stash
        // back into what it saves, so `updated_at` echoed the value the server already
        // had. The server's LWW check is `client > server`, so `X > X` was false and
        // every edit was silently discarded. A local edit must always win.
        let mut db = create_test_db();

        let stale = 1_000_000_u64;
        let stash = stash_with_updated_at("edit-me", "original", Some(stale));
        db.save_stash(&stash, None, WriteOrigin::LocalEdit)
            .expect("save should succeed");

        assert!(
            stored_updated_at(&db, "edit-me") > stale,
            "a local edit must stamp the current time, not echo the supplied value"
        );

        // Simulate the UI round-trip: read it back, change a field, save again.
        let loaded = db
            .get_stashes()
            .expect("load should succeed")
            .into_iter()
            .find(|s| s.id == "edit-me")
            .expect("stash should be present");
        let before = loaded.updated_at.expect("updated_at should be set");

        let mut edited = loaded;
        edited.content = "changed".to_string();
        db.save_stash(&edited, None, WriteOrigin::LocalEdit)
            .expect("second save should succeed");

        assert!(
            stored_updated_at(&db, "edit-me") >= before,
            "editing must never move updated_at backwards"
        );
    }

    #[test]
    fn sync_import_preserves_the_server_timestamp() {
        // The mirror of the rule above: data coming back from the server must keep its
        // timestamp verbatim, or every pulled record would look locally modified and be
        // pushed straight back on the next sync.
        let mut db = create_test_db();

        let server_ts = 1_700_000_000_u64;
        let stash = stash_with_updated_at("from-server", "remote content", Some(server_ts));
        db.import_stashes(&vec![stash]).expect("import should succeed");

        assert_eq!(
            stored_updated_at(&db, "from-server"),
            server_ts,
            "sync import must not restamp the server's timestamp"
        );
    }

    #[test]
    fn context_import_preserves_timestamp_and_description() {
        // `save_context` is the local-edit path; imports need their own so pulled
        // contexts keep the server clock. The description must survive too - it was
        // absent from the sync payload entirely, so every pull blanked it.
        let mut db = create_test_db();

        let server_ts = 1_700_000_000_u64;
        let ctx = Context {
            id: "ctx-remote".to_string(),
            name: "Remote".to_string(),
            description: Some("Rust + Svelte".to_string()),
            rules: vec![],
            last_used: None,
            updated_at: Some(server_ts),
            deleted: false,
        };

        db.import_contexts(&[ctx]).expect("import should succeed");

        let stored = db.get_contexts().expect("load should succeed");
        let found = stored
            .iter()
            .find(|c| c.id == "ctx-remote")
            .expect("context should be present");

        assert_eq!(found.updated_at, Some(server_ts));
        assert_eq!(found.description.as_deref(), Some("Rust + Svelte"));
    }

    #[test]
    fn local_context_edit_advances_updated_at() {
        let mut db = create_test_db();

        let stale = 1_000_000_u64;
        let ctx = Context {
            id: "ctx-local".to_string(),
            name: "Local".to_string(),
            description: None,
            rules: vec![],
            last_used: None,
            updated_at: Some(stale),
            deleted: false,
        };

        db.save_context(&ctx, WriteOrigin::LocalEdit)
            .expect("save should succeed");

        let stored = db.get_contexts().expect("load should succeed");
        let found = stored
            .iter()
            .find(|c| c.id == "ctx-local")
            .expect("context should be present");

        assert!(
            found.updated_at.unwrap() > stale,
            "a local context edit must stamp the current time"
        );
    }

    #[test]
    fn importing_a_stash_never_destroys_its_attachments() {
        // The data-loss regression: the server withholds attachments whose bytes are not
        // confirmed uploaded, so a sync moments after adding one returns that stash with
        // an empty attachment list. Combined with INSERT OR REPLACE - which deletes the
        // row, cascading to attachments - the file the user just attached was wiped
        // before it could ever be uploaded.
        let mut db = create_test_db();
        db.conn
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");

        let mut stash = stash_with_updated_at("s-att", "has a file", Some(1_700_000_000));
        stash.attachments = vec![Attachment {
            id: "att-1".to_string(),
            stash_id: "s-att".to_string(),
            file_path: "/cache/ctx/s-att/shot.png".to_string(),
            file_name: "shot.png".to_string(),
            file_size: 123,
            mime_type: Some("image/png".to_string()),
            syntax: None,
            created_at: "2026-08-19T10:00:00Z".to_string(),
        }];

        db.save_stash(&stash, None, WriteOrigin::LocalEdit)
            .expect("save should succeed");

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE stash_id = 's-att'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "attachment should exist after the local save");

        // Now the server echoes the stash back with no attachments, as it does until the
        // upload is confirmed.
        let mut from_server = stash.clone();
        from_server.attachments = vec![];
        from_server.content = "has a file".to_string();
        db.import_stashes(&vec![from_server])
            .expect("import should succeed");

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE stash_id = 's-att'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "importing a stash whose server copy lists no attachments must not delete them"
        );
    }

    #[test]
    fn import_preserves_a_local_file_path_against_an_empty_one() {
        // The server never sends file_path, so it deserializes to "". Writing that over a
        // real path would strand a file the device already holds on disk.
        let mut db = create_test_db();

        let mut stash = stash_with_updated_at("s-path", "content", Some(1_700_000_000));
        stash.attachments = vec![Attachment {
            id: "att-2".to_string(),
            stash_id: "s-path".to_string(),
            file_path: "/cache/ctx/s-path/real.png".to_string(),
            file_name: "real.png".to_string(),
            file_size: 10,
            mime_type: None,
            syntax: None,
            created_at: "2026-08-19T10:00:00Z".to_string(),
        }];
        db.save_stash(&stash, None, WriteOrigin::LocalEdit)
            .expect("save should succeed");

        // Same attachment arriving from the server, with no path.
        let mut from_server = stash.clone();
        from_server.attachments[0].file_path = String::new();
        db.import_stashes(&vec![from_server])
            .expect("import should succeed");

        let path: String = db
            .conn
            .query_row(
                "SELECT file_path FROM attachments WHERE id = 'att-2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            path, "/cache/ctx/s-path/real.png",
            "an empty incoming path means unknown, not cleared"
        );
    }

    fn pending_flag(db: &DbManager, table: &str, id: &str) -> i64 {
        db.conn
            .query_row(
                &format!("SELECT pending_sync FROM {} WHERE id = ?1", table),
                params![id],
                |r| r.get(0),
            )
            .expect("row should exist")
    }

    #[test]
    fn a_local_edit_marks_the_record_as_needing_a_push() {
        let mut db = create_test_db();
        let stash = stash_with_updated_at("s-dirty", "typed something", None);

        db.save_stash(&stash, None, WriteOrigin::LocalEdit)
            .expect("save should succeed");

        assert_eq!(pending_flag(&db, "stashes", "s-dirty"), 1);
        assert!(
            db.claim_pending_stashes()
                .expect("claim should succeed")
                .iter()
                .any(|s| s.id == "s-dirty"),
            "a locally edited stash must be queued for the next push"
        );
        assert_eq!(
            pending_flag(&db, "stashes", "s-dirty"),
            2,
            "claiming marks the record as in flight"
        );
    }

    #[test]
    fn data_arriving_from_the_server_is_not_queued_for_a_push() {
        // Otherwise every pulled record would bounce straight back, and the incremental
        // push would never shrink.
        let mut db = create_test_db();
        let stash = stash_with_updated_at("s-remote", "from another device", Some(1_700_000_000));

        db.import_stashes(&vec![stash]).expect("import should succeed");

        assert_eq!(pending_flag(&db, "stashes", "s-remote"), 0);
        // Scoped to this record: a fresh database also seeds starter stashes, which are
        // local creations and legitimately do need pushing.
        assert!(
            !db.claim_pending_stashes()
                .expect("claim")
                .iter()
                .any(|s| s.id == "s-remote"),
            "imported records are already in sync by definition"
        );
    }

    #[test]
    fn acknowledging_a_push_clears_only_what_was_sent() {
        let mut db = create_test_db();
        db.save_stash(
            &stash_with_updated_at("s-a", "one", None),
            None,
            WriteOrigin::LocalEdit,
        )
        .expect("save");
        db.save_stash(
            &stash_with_updated_at("s-b", "two", None),
            None,
            WriteOrigin::LocalEdit,
        )
        .expect("save");

        db.claim_pending_stashes().expect("claim");

        // Only s-a was accepted; s-b was rejected, so it is not acknowledged.
        db.mark_synced("stashes", &["s-a".to_string()])
            .expect("ack should succeed");

        assert_eq!(pending_flag(&db, "stashes", "s-a"), 0);
        assert_ne!(
            pending_flag(&db, "stashes", "s-b"),
            0,
            "a record the server did not accept must stay queued"
        );
        assert!(
            db.claim_pending_stashes()
                .expect("claim")
                .iter()
                .any(|s| s.id == "s-b"),
            "and must be picked up by the next push"
        );
    }

    #[test]
    fn an_edit_during_a_push_stays_queued() {
        // The race that would otherwise lose data silently: the user edits a stash while
        // the request carrying its previous version is still in flight. Acknowledging by
        // id alone would clear the flag and the newer edit would never be sent.
        let mut db = create_test_db();
        let stash = stash_with_updated_at("s-race", "original", None);
        db.save_stash(&stash, None, WriteOrigin::LocalEdit)
            .expect("save");

        // The push starts: the record is claimed and sent.
        db.claim_pending_stashes().expect("claim");
        assert_eq!(pending_flag(&db, "stashes", "s-race"), 2);

        // The user edits it again before the response arrives. Note this happens within
        // the same second, so `updated_at` may not have moved at all - which is exactly
        // why acknowledgement keys off the in-flight marker instead.
        let mut edited = stash.clone();
        edited.content = "edited mid-flight".to_string();
        db.save_stash(&edited, None, WriteOrigin::LocalEdit)
            .expect("save");

        // The response arrives and acknowledges what was sent.
        db.mark_synced("stashes", &["s-race".to_string()])
            .expect("ack");

        assert_eq!(
            pending_flag(&db, "stashes", "s-race"),
            1,
            "the newer edit must still be queued for the next push"
        );
        assert!(
            db.claim_pending_stashes()
                .expect("claim")
                .iter()
                .any(|s| s.id == "s-race"),
            "and must actually be included next time"
        );
    }

    #[test]
    fn a_failed_push_leaves_everything_queued() {
        // Nothing is acknowledged when the request errors, so the in-flight rows stay
        // pending and are re-claimed rather than silently dropped.
        let mut db = create_test_db();
        db.save_stash(
            &stash_with_updated_at("s-lost", "important", None),
            None,
            WriteOrigin::LocalEdit,
        )
        .expect("save");

        db.claim_pending_stashes().expect("claim");
        // ...request fails, so no mark_synced call happens at all.

        assert!(
            db.claim_pending_stashes()
                .expect("claim")
                .iter()
                .any(|s| s.id == "s-lost"),
            "a failed push must not lose the record"
        );
    }

    #[test]
    fn contexts_track_pending_state_the_same_way() {
        let mut db = create_test_db();
        let ctx = Context {
            id: "c-dirty".to_string(),
            name: "Project".to_string(),
            description: None,
            rules: vec![],
            last_used: None,
            updated_at: None,
            deleted: false,
        };

        db.save_context(&ctx, WriteOrigin::LocalEdit).expect("save");
        assert_eq!(pending_flag(&db, "contexts", "c-dirty"), 1);

        db.claim_pending_contexts().expect("claim");
        db.mark_synced("contexts", &["c-dirty".to_string()])
            .expect("ack");
        assert_eq!(pending_flag(&db, "contexts", "c-dirty"), 0);

        // And a pulled context is not queued.
        let mut remote = ctx.clone();
        remote.id = "c-remote".to_string();
        remote.updated_at = Some(1_700_000_000);
        db.import_contexts(&[remote]).expect("import");
        assert_eq!(pending_flag(&db, "contexts", "c-remote"), 0);
    }

    #[test]
    fn deleting_a_stash_queues_the_tombstone_for_the_push() {
        // A deletion only reaches other devices if it is actually pushed.
        let mut db = create_test_db();
        db.save_stash(
            &stash_with_updated_at("s-gone", "bye", None),
            None,
            WriteOrigin::LocalEdit,
        )
        .expect("save");
        db.claim_pending_stashes().expect("claim");
        db.mark_synced("stashes", &["s-gone".to_string()])
            .expect("ack");
        assert_eq!(pending_flag(&db, "stashes", "s-gone"), 0);

        db.delete_stash("s-gone").expect("delete");

        assert_eq!(
            pending_flag(&db, "stashes", "s-gone"),
            1,
            "the tombstone must be queued so other devices learn about the deletion"
        );
    }

    #[test]
    fn deleted_stashes_are_returned_to_sync_as_tombstones() {
        // Other devices only learn about a deletion if the tombstone is sent, so
        // get_stashes_for_sync must include deleted rows even though get_stashes hides
        // them from the UI.
        let mut db = create_test_db();

        let stash = stash_with_updated_at("doomed", "bye", None);
        db.save_stash(&stash, None, WriteOrigin::LocalEdit)
            .expect("save should succeed");
        db.delete_stash("doomed").expect("delete should succeed");

        let visible = db.get_stashes().expect("load should succeed");
        assert!(
            !visible.iter().any(|s| s.id == "doomed"),
            "deleted stash must be hidden from the UI"
        );

        let for_sync = db.get_stashes_for_sync().expect("sync load should succeed");
        let tombstone = for_sync
            .iter()
            .find(|s| s.id == "doomed")
            .expect("tombstone must be sent to the server");
        assert!(tombstone.deleted, "tombstone must be flagged as deleted");
    }
}
