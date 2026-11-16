use crate::blueprint::ProductBlueprint;
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
        let record = self
            .store
            .deployment(deployment_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(deployment_id.into()))?;
        let plan_json = self.store.plan_json(deployment_id)?;
        serde_json::from_str(&plan_json).map_err(|error| {
            ControlPlaneError::Plan(PlanError::InvalidImageReference(format!(
                "{}: {}",
                record.plan_hash, error
            )))
        })
    }

    pub fn deployment(
        &self,
        deployment_id: &str,
    ) -> Result<Option<DeploymentRecord>, ControlPlaneError> {
        Ok(self.store.deployment(deployment_id)?)
    }

    pub fn approve_deployment(
        &self,
        deployment_id: &str,
        plan_hash: &str,
        approved_by: &str,
    ) -> Result<(), ControlPlaneError> {
        self.store
            .approve_plan(deployment_id, plan_hash, approved_by)?;
        Ok(())
    }

    pub fn begin_execution(
        &self,
        deployment_id: &str,
    ) -> Result<OperationRecord, ControlPlaneError> {
        let deployment = self
            .store
            .deployment(deployment_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(deployment_id.into()))?;
        if deployment.approval_status != "approved" {
            return Err(ControlPlaneError::NotApproved);
        }
        let operation_id = format!("deploy:{deployment_id}");
        self.store
            .start_operation(&operation_id, deployment_id, "deploy")?;
        self.store.set_desired_state(deployment_id, "running")?;
        self.store
            .operation(&operation_id)?
            .ok_or_else(|| ControlPlaneError::NotFound(operation_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[test]
    fn creates_and_approves_a_persisted_plan() {
        let control = ControlPlane::new(registry(), StateStore::open_in_memory().unwrap());
        let deployment = control
            .plan_deployment("deployment-1", &blueprint())
            .unwrap();
        assert_eq!(deployment.plan.blueprint_id, "accounting");
        assert_eq!(deployment.record.approval_status, "pending");
        control
            .approve_deployment("deployment-1", &deployment.plan.plan_hash, "operator")
            .unwrap();
        assert_eq!(
            control
                .deployment("deployment-1")
                .unwrap()
                .unwrap()
                .approval_status,
            "approved"
        );
    }
    #[test]
    fn refuses_execution_until_matching_plan_is_approved() {
        let control = ControlPlane::new(registry(), StateStore::open_in_memory().unwrap());
        control
            .plan_deployment("deployment-1", &blueprint())
            .unwrap();
        assert!(matches!(
            control.begin_execution("deployment-1"),
            Err(ControlPlaneError::NotApproved)
        ));
        let hash = control.plan("deployment-1").unwrap().plan_hash;
        control
            .approve_deployment("deployment-1", &hash, "operator")
            .unwrap();
        assert_eq!(
            control.begin_execution("deployment-1").unwrap().status,
            "queued"
        );
    }
}
