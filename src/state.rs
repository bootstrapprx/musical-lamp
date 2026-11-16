use crate::planner::DeploymentPlan;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug)]
pub enum StoreError {
    Sql(rusqlite::Error),
    Poisoned,
    ActiveOperation { deployment_id: String },
    PlanHashMismatch,
    AlreadyApproved,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentRecord {
    pub id: String,
    pub blueprint_id: String,
    pub blueprint_revision: u64,
    pub plan_hash: String,
    pub desired_state: String,
    pub observed_state: String,
    pub approval_status: String,
    pub approved_by: Option<String>,
    pub approval_operation_id: Option<String>,
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
                 plan_json TEXT NOT NULL DEFAULT '{}',
                 plan_hash TEXT NOT NULL DEFAULT '',
                 desired_state TEXT NOT NULL DEFAULT 'pending',
                 observed_state TEXT NOT NULL DEFAULT 'unknown',
                 approval_status TEXT NOT NULL DEFAULT 'pending',
                 approved_by TEXT,
                 approval_operation_id TEXT,
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
        self.create_deployment_with_plan(id, blueprint_id, revision, None)
    }

    pub fn create_deployment_with_plan(
        &self,
        id: &str,
        blueprint_id: &str,
        revision: u64,
        plan: Option<&DeploymentPlan>,
    ) -> Result<(), StoreError> {
        let (plan_json, plan_hash) = match plan {
            Some(plan) => (
                serde_json::to_string(plan).map_err(|error| {
                    StoreError::Sql(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
                })?,
                plan.plan_hash.clone(),
            ),
            None => ("{}".into(), String::new()),
        };
        self.create_deployment_with_metadata(id, blueprint_id, revision, &plan_json, &plan_hash)
    }

    pub fn create_deployment_with_metadata(
        &self,
        id: &str,
        blueprint_id: &str,
        revision: u64,
        plan_json: &str,
        plan_hash: &str,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection.execute(
            "INSERT INTO deployments(id, blueprint_id, blueprint_revision, plan_json, plan_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, blueprint_id, revision as i64, plan_json, plan_hash],
        )?;
        Ok(())
    }

    pub fn deployment(&self, id: &str) -> Result<Option<DeploymentRecord>, StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection
            .query_row(
                "SELECT id, blueprint_id, blueprint_revision, plan_hash, desired_state, observed_state, approval_status, approved_by, approval_operation_id FROM deployments WHERE id = ?1",
                params![id],
                |row| Ok(DeploymentRecord {
                    id: row.get(0)?, blueprint_id: row.get(1)?, blueprint_revision: row.get::<_, i64>(2)? as u64,
                    plan_hash: row.get(3)?, desired_state: row.get(4)?, observed_state: row.get(5)?,
                    approval_status: row.get(6)?, approved_by: row.get(7)?, approval_operation_id: row.get(8)?,
                }),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn approve_plan(
        &self,
        deployment_id: &str,
        expected_plan_hash: &str,
        approved_by: &str,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        let (actual_hash, approval_status): (String, String) = connection.query_row(
            "SELECT plan_hash, approval_status FROM deployments WHERE id = ?1",
            params![deployment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if approval_status == "approved" {
            return Err(StoreError::AlreadyApproved);
        }
        if actual_hash != expected_plan_hash {
            return Err(StoreError::PlanHashMismatch);
        }
        let operation_id = format!("approval:{deployment_id}");
        connection.execute(
            "INSERT INTO operations(id, deployment_id, kind, status) VALUES (?1, ?2, 'approve_plan', 'succeeded')",
            params![operation_id, deployment_id],
        )?;
        connection.execute(
            "UPDATE deployments SET approval_status = 'approved', approved_by = ?2, approval_operation_id = ?3 WHERE id = ?1",
            params![deployment_id, approved_by, operation_id],
        )?;
        Ok(())
    }

    pub fn set_desired_state(&self, deployment_id: &str, state: &str) -> Result<(), StoreError> {
        self.set_state_column(deployment_id, "desired_state", state)
    }

    pub fn set_observed_state(&self, deployment_id: &str, state: &str) -> Result<(), StoreError> {
        self.set_state_column(deployment_id, "observed_state", state)
    }

    fn set_state_column(
        &self,
        deployment_id: &str,
        column: &str,
        state: &str,
    ) -> Result<(), StoreError> {
        let query = format!("UPDATE deployments SET {column} = ?2 WHERE id = ?1");
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        connection.execute(&query, params![deployment_id, state])?;
        Ok(())
    }

    pub fn start_operation(
        &self,
        id: &str,
        deployment_id: &str,
        kind: &str,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().map_err(|_| StoreError::Poisoned)?;
        let active: Option<String> = connection.query_row(
            "SELECT id FROM operations WHERE deployment_id = ?1 AND status IN ('queued', 'running') LIMIT 1",
            params![deployment_id], |row| row.get(0)).optional()?;
        if active.is_some() {
            return Err(StoreError::ActiveOperation {
                deployment_id: deployment_id.into(),
            });
        }
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
        connection.execute("INSERT INTO operation_events(operation_id, sequence, status, message) VALUES (?1, ?2, ?3, ?4)", params![operation_id, sequence, status, message])?;
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
    fn persists_plan_and_separates_desired_from_observed_state() {
        let store = StateStore::open_in_memory().unwrap();
        store
            .create_deployment("deployment-1", "blueprint-1", 3)
            .unwrap();
        store.set_desired_state("deployment-1", "running").unwrap();
        store
            .set_observed_state("deployment-1", "degraded")
            .unwrap();
        let deployment = store.deployment("deployment-1").unwrap().unwrap();
        assert_eq!(deployment.desired_state, "running");
        assert_eq!(deployment.observed_state, "degraded");
        assert_eq!(deployment.plan_hash, "");
    }

    #[test]
    fn approval_requires_matching_plan_hash_and_is_audited() {
        let store = StateStore::open_in_memory().unwrap();
        store
            .create_deployment_with_metadata(
                "deployment-1",
                "blueprint-1",
                3,
                "{\"plan\":true}",
                "plan-hash-1",
            )
            .unwrap();

        assert!(matches!(
            store.approve_plan("deployment-1", "wrong-hash", "operator"),
            Err(StoreError::PlanHashMismatch)
        ));
        store
            .approve_plan("deployment-1", "plan-hash-1", "operator")
            .unwrap();
        let deployment = store.deployment("deployment-1").unwrap().unwrap();
        assert_eq!(deployment.approval_status, "approved");
        assert_eq!(deployment.approved_by.as_deref(), Some("operator"));
        assert_eq!(
            deployment.approval_operation_id.as_deref(),
            Some("approval:deployment-1")
        );
        assert_eq!(
            store
                .operation("approval:deployment-1")
                .unwrap()
                .unwrap()
                .status,
            "succeeded"
        );
    }

    #[test]
    fn approval_cannot_be_repeated_after_the_plan_is_approved() {
        let store = StateStore::open_in_memory().unwrap();
        store
            .create_deployment_with_metadata("deployment-1", "blueprint-1", 1, "{}", "hash")
            .unwrap();
        store
            .approve_plan("deployment-1", "hash", "operator")
            .unwrap();
        assert!(matches!(
            store.approve_plan("deployment-1", "hash", "operator"),
            Err(StoreError::AlreadyApproved)
        ));
    }

    #[test]
    fn rejects_conflicting_active_operations_but_allows_after_finish() {
        let store = StateStore::open_in_memory().unwrap();
        store
            .create_deployment("deployment-1", "blueprint-1", 1)
            .unwrap();
        store
            .start_operation("op-1", "deployment-1", "deploy")
            .unwrap();
        assert!(matches!(
            store.start_operation("op-2", "deployment-1", "migrate"),
            Err(StoreError::ActiveOperation { .. })
        ));
        store.finish_operation("op-1", "succeeded").unwrap();
        store
            .start_operation("op-2", "deployment-1", "migrate")
            .unwrap();
    }

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
}
