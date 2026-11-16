use crate::blueprint::ProductBlueprint;
use crate::executor::{validate_local_host, CommandExecutor, CommandSpec, HostCheck};
use crate::planner::{DeploymentPlan, DeploymentPlanner, PlanError};
use crate::registry::AppRegistry;
use crate::state::{DeploymentRecord, OperationRecord, StateStore, StoreError};
use std::sync::Arc;

#[derive(Debug)]
pub enum ControlPlaneError {
    Plan(PlanError),
    Store(StoreError),
    NotFound(String),
    NotApproved,
    HostValidationFailed,
    InfrastructureFailed,
    RuntimeProvisioningFailed,
}
impl From<PlanError> for ControlPlaneError {
    fn from(error: PlanError) -> Self {
        Self::Plan(error)
    }
}
impl From<StoreError> for ControlPlaneError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

#[derive(Debug, Clone)]
pub struct PlannedDeployment {
    pub plan: DeploymentPlan,
    pub record: DeploymentRecord,
}

pub struct ControlPlane {
    registry: AppRegistry,
    store: Arc<StateStore>,
}

impl ControlPlane {
    pub fn new(registry: AppRegistry, store: StateStore) -> Self {
        Self {
            registry,
            store: Arc::new(store),
        }
    }
    pub fn plan_deployment(
        &self,
        deployment_id: &str,
        blueprint: &ProductBlueprint,
    ) -> Result<PlannedDeployment, ControlPlaneError> {
        let plan = DeploymentPlanner::new(&self.registry).plan(blueprint)?;
        self.store.create_deployment_with_plan(
            deployment_id,
            &blueprint.metadata.id,
            blueprint.metadata.revision,
            Some(&plan),
        )?;
        let record = self
            .store
            .deployment(deployment_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(deployment_id.into()))?;
        Ok(PlannedDeployment { plan, record })
    }
    pub fn plan(&self, deployment_id: &str) -> Result<DeploymentPlan, ControlPlaneError> {
        self.store
            .deployment(deployment_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(deployment_id.into()))?;
        let json = self.store.plan_json(deployment_id)?;
        serde_json::from_str(&json)
            .map_err(|e| ControlPlaneError::Plan(PlanError::InvalidImageReference(e.to_string())))
    }
    pub fn deployment(&self, id: &str) -> Result<Option<DeploymentRecord>, ControlPlaneError> {
        Ok(self.store.deployment(id)?)
    }
    pub fn operation(&self, id: &str) -> Result<Option<OperationRecord>, ControlPlaneError> {
        Ok(self.store.operation(id)?)
    }
    pub fn approve_deployment(
        &self,
        id: &str,
        hash: &str,
        actor: &str,
    ) -> Result<(), ControlPlaneError> {
        self.store.approve_plan(id, hash, actor)?;
        Ok(())
    }
    pub fn begin_execution(&self, id: &str) -> Result<OperationRecord, ControlPlaneError> {
        let deployment = self
            .store
            .deployment(id)?
            .ok_or_else(|| ControlPlaneError::NotFound(id.into()))?;
        if deployment.approval_status != "approved" {
            return Err(ControlPlaneError::NotApproved);
        }
        let operation_id = format!("deploy:{id}");
        self.store.start_operation(&operation_id, id, "deploy")?;
        self.store.set_desired_state(id, "running")?;
        self.store
            .operation(&operation_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(operation_id))
    }
    pub fn execute_prepare_infrastructure(
        &self,
        id: &str,
        executor: &dyn CommandExecutor,
    ) -> Result<OperationRecord, ControlPlaneError> {
        let deployment = self
            .store
            .deployment(id)?
            .ok_or_else(|| ControlPlaneError::NotFound(id.into()))?;
        if deployment.approval_status != "approved" {
            return Err(ControlPlaneError::NotApproved);
        }
        let plan = self.plan(id)?;
        let operation_id = format!("infrastructure:{id}");
        self.store
            .start_operation(&operation_id, id, "prepare_infrastructure")?;
        self.store.append_event(
            &operation_id,
            "running",
            "infrastructure preparation started",
        )?;

        let commands = [
            crate::executor::CommandSpec {
                program: "docker".into(),
                args: vec!["pull".into(), plan.database_image.clone()],
                working_dir: None,
            },
            crate::executor::CommandSpec {
                program: "docker".into(),
                args: vec!["pull".into(), plan.cache_image.clone()],
                working_dir: None,
            },
        ];
        for command in commands {
            if executor
                .run(&command, std::time::Duration::from_secs(30))
                .is_err()
            {
                self.store.append_event(
                    &operation_id,
                    "failed",
                    "infrastructure preparation failed",
                )?;
                self.store.finish_operation(&operation_id, "failed")?;
                self.store.set_observed_state(id, "degraded")?;
                return Err(ControlPlaneError::InfrastructureFailed);
            }
        }
        self.store
            .append_event(&operation_id, "succeeded", "infrastructure prepared")?;
        self.store.finish_operation(&operation_id, "succeeded")?;
        self.store.set_observed_state(id, "infrastructure_ready")?;
        self.store
            .operation(&operation_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(operation_id))
    }

    pub fn execute_runtime_provisioning(
        &self,
        id: &str,
        executor: &dyn CommandExecutor,
    ) -> Result<OperationRecord, ControlPlaneError> {
        let deployment = self
            .store
            .deployment(id)?
            .ok_or_else(|| ControlPlaneError::NotFound(id.into()))?;
        if deployment.approval_status != "approved" {
            return Err(ControlPlaneError::NotApproved);
        }
        let operation_id = format!("runtime:{id}");
        self.store
            .start_operation(&operation_id, id, "runtime_provisioning")?;
        self.store
            .append_event(&operation_id, "running", "runtime provisioning started")?;
        let project = format!("musical-lamp-{id}");
        let commands = [
            CommandSpec {
                program: "docker".into(),
                args: vec!["network".into(), "create".into(), format!("{project}-app")],
                working_dir: None,
            },
            CommandSpec {
                program: "docker".into(),
                args: vec![
                    "volume".into(),
                    "create".into(),
                    format!("{project}-db-data"),
                ],
                working_dir: None,
            },
            CommandSpec {
                program: "docker".into(),
                args: vec!["volume".into(), "create".into(), format!("{project}-sites")],
                working_dir: None,
            },
        ];
        for command in commands {
            if executor
                .run(&command, std::time::Duration::from_secs(30))
                .is_err()
            {
                self.store
                    .append_event(&operation_id, "failed", "runtime provisioning failed")?;
                self.store.finish_operation(&operation_id, "failed")?;
                self.store.set_observed_state(id, "degraded")?;
                return Err(ControlPlaneError::RuntimeProvisioningFailed);
            }
        }
        self.store
            .append_event(&operation_id, "succeeded", "runtime resources provisioned")?;
        self.store.finish_operation(&operation_id, "succeeded")?;
        self.store.set_observed_state(id, "runtime_ready")?;
        self.store
            .operation(&operation_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(operation_id))
    }

    pub fn execute_host_validation(
        &self,
        id: &str,
        executor: &dyn CommandExecutor,
    ) -> Result<OperationRecord, ControlPlaneError> {
        let deployment = self
            .store
            .deployment(id)?
            .ok_or_else(|| ControlPlaneError::NotFound(id.into()))?;
        if deployment.approval_status != "approved" {
            return Err(ControlPlaneError::NotApproved);
        }
        let operation_id = format!("host:{id}");
        self.store
            .start_operation(&operation_id, id, "host_validation")?;
        self.store
            .append_event(&operation_id, "running", "host validation started")?;
        let checks = validate_local_host(executor);
        if checks.iter().all(|check| matches!(check, HostCheck::Ready)) {
            self.store
                .append_event(&operation_id, "succeeded", "host validation passed")?;
            self.store.finish_operation(&operation_id, "succeeded")?;
            self.store.set_observed_state(id, "ready")?;
            return self
                .store
                .operation(&operation_id)?
                .ok_or_else(|| ControlPlaneError::NotFound(operation_id));
        }
        self.store
            .append_event(&operation_id, "failed", "host validation failed")?;
        self.store.finish_operation(&operation_id, "failed")?;
        self.store.set_observed_state(id, "degraded")?;
        Err(ControlPlaneError::HostValidationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{CommandResult, ExecutorError, FakeExecutor};
    use crate::registry::AppRegistry;
    fn registry() -> AppRegistry {
        AppRegistry::from_yaml(r#"
api_version: musical-lamp/v1
kind: AppRegistry
apps:
  - { id: frappe, name: Frappe Framework, kind: framework, source: { repository: https://github.com/frappe/frappe.git, ref: v16.9.0, resolved_commit: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa }, dependencies: [], compatibility: { framework_major: "16" }, installation: { order: 0 }, configuration: { fields: [] } }
  - { id: erpnext, name: ERPNext, kind: application, source: { repository: https://github.com/frappe/erpnext.git, ref: v16.9.1, resolved_commit: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb }, dependencies: [], compatibility: { framework: frappe, framework_major: "16" }, installation: { order: 10 }, configuration: { fields: [] } }
"#).unwrap()
    }
    fn blueprint() -> ProductBlueprint {
        ProductBlueprint::from_yaml(r#"
api_version: musical-lamp/v1
kind: ProductBlueprint
metadata: { id: accounting, name: Accounting, revision: 1 }
framework: { app: frappe }
apps: [{ app: erpnext }]
sites: [{ name: accounting.example.invalid }]
runtime:
  image: { policy: pull, repository: registry.example/accounting, reference: registry.example/accounting@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa }
  database: { engine: mariadb, image: mariadb@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa }
  cache: { engine: redis, image: redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb }
"#).unwrap()
    }
    fn approved_control() -> ControlPlane {
        let c = ControlPlane::new(registry(), StateStore::open_in_memory().unwrap());
        let p = c.plan_deployment("deployment-1", &blueprint()).unwrap();
        c.approve_deployment("deployment-1", &p.plan.plan_hash, "operator")
            .unwrap();
        c
    }
    #[test]
    fn creates_and_approves_a_persisted_plan() {
        let c = approved_control();
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .approval_status,
            "approved"
        );
    }
    #[test]
    fn refuses_execution_until_matching_plan_is_approved() {
        let c = ControlPlane::new(registry(), StateStore::open_in_memory().unwrap());
        c.plan_deployment("deployment-1", &blueprint()).unwrap();
        assert!(matches!(
            c.begin_execution("deployment-1"),
            Err(ControlPlaneError::NotApproved)
        ));
        let hash = c.plan("deployment-1").unwrap().plan_hash;
        c.approve_deployment("deployment-1", &hash, "operator")
            .unwrap();
        assert_eq!(c.begin_execution("deployment-1").unwrap().status, "queued");
    }
    #[test]
    fn executes_host_validation_and_persists_success_events() {
        let c = approved_control();
        let e = FakeExecutor::default();
        e.respond(
            "docker",
            Ok(CommandResult {
                status: 0,
                stdout: "Docker".into(),
                stderr: String::new(),
            }),
        );
        e.respond(
            "bash",
            Ok(CommandResult {
                status: 0,
                stdout: "Bash".into(),
                stderr: String::new(),
            }),
        );
        let op = c.execute_host_validation("deployment-1", &e).unwrap();
        assert_eq!(op.status, "succeeded");
        assert_eq!(
            op.events,
            vec!["host validation started", "host validation passed"]
        );
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .observed_state,
            "ready"
        );
    }
    #[test]
    fn prepares_infrastructure_from_persisted_plan_and_updates_observed_state() {
        let c = approved_control();
        let e = FakeExecutor::default();
        e.respond(
            "docker",
            Ok(CommandResult {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        );
        let op = c
            .execute_prepare_infrastructure("deployment-1", &e)
            .unwrap();
        assert_eq!(op.status, "succeeded");
        assert_eq!(
            op.events,
            vec![
                "infrastructure preparation started",
                "infrastructure prepared"
            ]
        );
        let calls = e.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].args[0], "pull");
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .observed_state,
            "infrastructure_ready"
        );
    }

    #[test]
    fn infrastructure_failure_marks_operation_and_deployment_degraded() {
        let c = approved_control();
        let e = FakeExecutor::default();
        e.respond(
            "docker",
            Err(ExecutorError::Failed(CommandResult {
                status: 1,
                stdout: String::new(),
                stderr: "pull failed".into(),
            })),
        );
        assert!(matches!(
            c.execute_prepare_infrastructure("deployment-1", &e),
            Err(ControlPlaneError::InfrastructureFailed)
        ));
        assert_eq!(
            c.operation("infrastructure:deployment-1")
                .unwrap()
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .observed_state,
            "degraded"
        );
    }

    #[test]
    fn provisions_runtime_resources_from_deployment_identity() {
        let c = approved_control();
        let e = FakeExecutor::default();
        e.respond(
            "docker",
            Ok(CommandResult {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        );
        let operation = c.execute_runtime_provisioning("deployment-1", &e).unwrap();
        assert_eq!(operation.status, "succeeded");
        let calls = e.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0].args,
            vec!["network", "create", "musical-lamp-deployment-1-app"]
        );
        assert_eq!(
            calls[1].args,
            vec!["volume", "create", "musical-lamp-deployment-1-db-data"]
        );
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .observed_state,
            "runtime_ready"
        );
    }

    #[test]
    fn runtime_provisioning_failure_is_persisted() {
        let c = approved_control();
        let e = FakeExecutor::default();
        e.respond(
            "docker",
            Err(ExecutorError::Failed(CommandResult {
                status: 1,
                stdout: String::new(),
                stderr: "create failed".into(),
            })),
        );
        assert!(matches!(
            c.execute_runtime_provisioning("deployment-1", &e),
            Err(ControlPlaneError::RuntimeProvisioningFailed)
        ));
        assert_eq!(
            c.operation("runtime:deployment-1").unwrap().unwrap().status,
            "failed"
        );
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .observed_state,
            "degraded"
        );
    }

    #[test]
    fn failed_host_validation_marks_deployment_degraded() {
        let c = approved_control();
        let e = FakeExecutor::default();
        e.respond(
            "docker",
            Err(ExecutorError::Spawn("daemon unavailable".into())),
        );
        e.respond(
            "bash",
            Ok(CommandResult {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        );
        assert!(matches!(
            c.execute_host_validation("deployment-1", &e),
            Err(ControlPlaneError::HostValidationFailed)
        ));
        assert_eq!(
            c.operation("host:deployment-1").unwrap().unwrap().status,
            "failed"
        );
        assert_eq!(
            c.deployment("deployment-1")
                .unwrap()
                .unwrap()
                .observed_state,
            "degraded"
        );
    }
}
