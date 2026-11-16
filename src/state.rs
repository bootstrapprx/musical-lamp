use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Poisoned,
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRecord {
    pub id: String,
    pub deployment_id: String,
    pub kind: String,
    pub status: String,
    pub events: Vec<String>,
}

pub struct StateStore {
    connection: Mutex<Connection>,
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn migrate(&self) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS schema_migrations (
                 version INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS deployments (
                 id TEXT PRIMARY KEY,
                 blueprint_id TEXT NOT NULL,
                 blueprint_revision INTEGER NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             CREATE TABLE IF NOT EXISTS operations (
                 id TEXT PRIMARY KEY,
                 deployment_id TEXT NOT NULL REFERENCES deployments(id),
                 kind TEXT NOT NULL,
                 status TEXT NOT NULL,
                 started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 finished_at TEXT
             );
             CREATE TABLE IF NOT EXISTS operation_events (
                 operation_id TEXT NOT NULL REFERENCES operations(id),
                 sequence INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 message TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                 PRIMARY KEY (operation_id, sequence)
             );
             INSERT OR IGNORE INTO schema_migrations(version) VALUES (1);",
        )?;
        Ok(())
    }

    pub fn create_deployment(
        &self,
        id: &str,
        blueprint_id: &str,
        revision: u64,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection.execute(
            "INSERT INTO deployments(id, blueprint_id, blueprint_revision) VALUES (?1, ?2, ?3)",
            params![id, blueprint_id, revision as i64],
        )?;
        Ok(())
    }

    pub fn start_operation(
        &self,
        id: &str,
        deployment_id: &str,
        kind: &str,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection.execute(
            "INSERT INTO operations(id, deployment_id, kind, status) VALUES (?1, ?2, ?3, 'queued')",
            params![id, deployment_id, kind],
        )?;
        Ok(())
    }

    pub fn append_event(
        &self,
        operation_id: &str,
        status: &str,
        message: &str,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        let sequence: i64 = connection.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM operation_events WHERE operation_id = ?1",
            params![operation_id],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT INTO operation_events(operation_id, sequence, status, message) VALUES (?1, ?2, ?3, ?4)",
            params![operation_id, sequence, status, message],
        )?;
        connection.execute(
            "UPDATE operations SET status = ?2 WHERE id = ?1",
            params![operation_id, status],
        )?;
        Ok(())
    }

    pub fn finish_operation(&self, operation_id: &str, status: &str) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection.execute(
            "UPDATE operations SET status = ?2, finished_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![operation_id, status],
        )?;
        Ok(())
    }

    pub fn operation(&self, id: &str) -> Result<Option<OperationRecord>, StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        let Some((operation_id, deployment_id, kind, status)) = connection
            .query_row(
                "SELECT id, deployment_id, kind, status FROM operations WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
        else {
            return Ok(None);
        };
        let mut statement = connection.prepare(
            "SELECT message FROM operation_events WHERE operation_id = ?1 ORDER BY sequence",
        )?;
        let events = statement
            .query_map(params![id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(Some(OperationRecord {
            id: operation_id,
            deployment_id,
            kind,
            status,
            events,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_schema_and_round_trips_operation_history() {
        let store = StateStore::open_in_memory().unwrap();
        store
            .create_deployment("deployment-1", "blueprint-1", 3)
            .unwrap();
        store
            .start_operation("op-1", "deployment-1", "deploy")
            .unwrap();
        store
            .append_event("op-1", "running", "preparing infrastructure")
            .unwrap();
        store.finish_operation("op-1", "succeeded").unwrap();
        let operation = store.operation("op-1").unwrap().unwrap();
        assert_eq!(operation.status, "succeeded");
        assert_eq!(operation.events, vec!["preparing infrastructure"]);
    }

    #[test]
    fn migration_is_idempotent_and_rejects_duplicate_operation_ids() {
        let store = StateStore::open_in_memory().unwrap();
        store.migrate().unwrap();
        store.migrate().unwrap();
        store
            .create_deployment("deployment-1", "blueprint-1", 1)
            .unwrap();
        store
            .start_operation("op-1", "deployment-1", "plan")
            .unwrap();
        let error = store
            .start_operation("op-1", "deployment-1", "plan")
            .unwrap_err();
        assert!(matches!(error, StoreError::Sql(_)));
    }
}
