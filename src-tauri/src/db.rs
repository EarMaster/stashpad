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

    pub fn migrate_from_json(&mut self, stashes: Vec<crate::StashItem>, contexts: Vec<crate::Context>) -> Result<()> {
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
        let mut stmt = self.conn.prepare("SELECT id, name, rules, last_used, description FROM contexts WHERE deleted = 0")?;
        let rows = stmt.query_map([], |row| {
            let rules_str: String = row.get(2)?;
            let rules: Vec<ContextRule> = serde_json::from_str(&rules_str).unwrap_or_default();
            Ok(Context {
                id: row.get(0)?,
                name: row.get(1)?,
                rules,
                last_used: row.get(3)?,
                description: row.get(4)?,
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
            "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted, description) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                ctx.id,
                name,
                rules_json,
                ctx.last_used,
                now_ts(),
                ctx.description
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
        let mut stmt = self.conn.prepare("SELECT id, context_id, content, files, created_at, completed, completed_at, position, enhanced_content FROM stashes WHERE deleted = 0 ORDER BY position ASC")?;
        
        let stash_rows = stmt.query_map([], |row| {
            let files_str: String = row.get(3)?;
            // files_str kept for backward compat or if needed, but we now use attachments table.
            // We'll populate attachments below.
            
            Ok(StashItem {
                id: row.get(0)?,
                context_id: row.get(1)?,
                content: row.get(2)?,
                enhanced_content: row.get(8)?,
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

    pub fn save_stash(&mut self, stash: &StashItem, position: Option<f64>) -> Result<()> {
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
            "INSERT OR REPLACE INTO stashes (id, context_id, content, enhanced_content, files, created_at, completed, completed_at, position, updated_at, deleted) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
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
                }
            ],
            last_used: Some(chrono::Utc::now().to_rfc3339()),
            description: Some("Test description".to_string()),
        };
        
        db.save_context(&test_context).expect("Failed to save context");
        
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
        };
        
        db.save_context(&modified_default).expect("Save should succeed");
        
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
        };
        
        db.save_stash(&stash, None).expect("Failed to save stash");
        
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
        };
        
        db.save_stash(&stash, None).expect("Failed to save stash");
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
        };
        
        db.save_stash(&completed, None).expect("Failed to save completed stash");
        db.save_stash(&active, None).expect("Failed to save active stash");
        
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
        };
        
        // Save without explicit position (should append)
        db.save_stash(&stash1, None).expect("Failed to save stash1");
        db.save_stash(&stash2, None).expect("Failed to save stash2");
        
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
}
