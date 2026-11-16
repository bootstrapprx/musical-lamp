use crate::blueprint::{ImagePolicy, ProductBlueprint};
use crate::registry::AppRegistry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentPlan {
    pub blueprint_id: String,
    pub blueprint_revision: u64,
    pub framework: ResolvedSource,
    pub applications: Vec<PlannedApplication>,
    pub compatibility: Vec<CompatibilityCheck>,
    pub image_policy: ImagePlanPolicy,
    pub image_reference: Option<String>,
    pub database_image: String,
    pub cache_image: String,
    pub sites: Vec<String>,
    pub steps: Vec<PlanStep>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedSource {
    pub app: String,
    pub repository: String,
    pub source_ref: String,
    pub resolved_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedApplication {
    pub app: String,
    pub source: ResolvedSource,
    pub dependencies: Vec<String>,
    pub install_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityCheck {
    pub app: String,
    pub status: CompatibilityStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompatibilityStatus {
    Passed,
    Overridden,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImagePlanPolicy {
    Build,
    Pull,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub kind: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    Composition(crate::blueprint::CompositionError),
    SourceNotResolved { app: String },
    Incompatible { app: String, detail: String },
    CompatibilityUnknown { app: String },
    InvalidImageReference(String),
}

pub struct DeploymentPlanner<'a> {
    registry: &'a AppRegistry,
    compatibility_overrides: HashMap<String, String>,
}

impl<'a> DeploymentPlanner<'a> {
    pub fn new(registry: &'a AppRegistry) -> Self {
        Self {
            registry,
            compatibility_overrides: HashMap::new(),
        }
    }

    pub fn with_compatibility_override(mut self, app: &str, justification: &str) -> Self {
        self.compatibility_overrides
            .insert(app.to_string(), justification.to_string());
        self
    }

    pub fn plan(&self, blueprint: &ProductBlueprint) -> Result<DeploymentPlan, PlanError> {
        let composition = blueprint
            .resolve(self.registry)
            .map_err(PlanError::Composition)?;
        let framework_def = self
            .registry
            .apps
            .iter()
            .find(|a| a.id == composition.framework)
            .expect("validated composition framework");
        let framework = resolved_source(framework_def)?;
        let mut compatibility = vec![CompatibilityCheck {
            app: framework_def.id.clone(),
            status: CompatibilityStatus::Passed,
            detail: "framework selected explicitly".into(),
        }];
        let mut applications = Vec::new();

        for app_id in &composition.applications {
            let app = self
                .registry
                .installable(app_id)
                .expect("validated composition app");
            compatibility.push(self.check_compatibility(app, &composition.framework)?);
            applications.push(PlannedApplication {
                app: app.id.clone(),
                source: resolved_source(app)?,
                dependencies: app.dependencies.clone(),
                install_order: app.installation.order,
            });
        }

        let (image_policy, image_reference) = match blueprint.runtime.image.policy {
            ImagePolicy::Build => (ImagePlanPolicy::Build, None),
            ImagePolicy::Pull => {
                let reference = blueprint.runtime.image.reference.clone().ok_or_else(|| {
                    PlanError::InvalidImageReference("pull policy requires image.reference".into())
                })?;
                validate_digest(&reference)?;
                (ImagePlanPolicy::Pull, Some(reference))
            }
        };
        validate_digest(&blueprint.runtime.database.image)?;
        validate_digest(&blueprint.runtime.cache.image)?;

        let steps = vec![
            PlanStep {
                id: "validate-host".into(),
                kind: "host_validation".into(),
                depends_on: vec![],
            },
            PlanStep {
                id: "prepare-infrastructure".into(),
                kind: "prepare_infrastructure".into(),
                depends_on: vec!["validate-host".into()],
            },
            PlanStep {
                id: "create-sites".into(),
                kind: "create_sites".into(),
                depends_on: vec!["prepare-infrastructure".into()],
            },
            PlanStep {
                id: "install-applications".into(),
                kind: "install_applications".into(),
                depends_on: vec!["create-sites".into()],
            },
            PlanStep {
                id: "verify-health".into(),
                kind: "verify_health".into(),
                depends_on: vec!["install-applications".into()],
            },
        ];
        let mut plan = DeploymentPlan {
            blueprint_id: blueprint.metadata.id.clone(),
            blueprint_revision: blueprint.metadata.revision,
            framework,
            applications,
            compatibility,
            image_policy,
            image_reference,
            database_image: blueprint.runtime.database.image.clone(),
            cache_image: blueprint.runtime.cache.image.clone(),
            sites: blueprint
                .sites
                .iter()
                .map(|site| site.name.clone())
                .collect(),
            steps,
            plan_hash: String::new(),
        };
        plan.plan_hash = stable_hash(&plan)?;
        Ok(plan)
    }

    fn check_compatibility(
        &self,
        app: &crate::registry::AppDefinition,
        framework: &str,
    ) -> Result<CompatibilityCheck, PlanError> {
        let Some(map) = app.compatibility.as_mapping() else {
            return self.override_or_unknown(&app.id);
        };
        let Some(required) = map
            .get(serde_yaml::Value::String("framework".into()))
            .and_then(|v| v.as_str())
        else {
            return self.override_or_unknown(&app.id);
        };
        if required != framework {
            return Err(PlanError::Incompatible {
                app: app.id.clone(),
                detail: format!("requires framework {required}, selected {framework}"),
            });
        }
        Ok(CompatibilityCheck {
            app: app.id.clone(),
            status: CompatibilityStatus::Passed,
            detail: format!("compatible with framework {framework}"),
        })
    }

    fn override_or_unknown(&self, app: &str) -> Result<CompatibilityCheck, PlanError> {
        if let Some(reason) = self.compatibility_overrides.get(app) {
            return Ok(CompatibilityCheck {
                app: app.into(),
                status: CompatibilityStatus::Overridden,
                detail: reason.clone(),
            });
        }
        Err(PlanError::CompatibilityUnknown { app: app.into() })
    }
}

fn resolved_source(app: &crate::registry::AppDefinition) -> Result<ResolvedSource, PlanError> {
    Ok(ResolvedSource {
        app: app.id.clone(),
        repository: app.source.repository.clone(),
        source_ref: app.source.source_ref.clone(),
        resolved_commit: app.source.resolved_commit.clone().ok_or_else(|| {
            PlanError::SourceNotResolved {
                app: app.id.clone(),
            }
        })?,
    })
}

fn validate_digest(value: &str) -> Result<(), PlanError> {
    let Some((_, digest)) = value.split_once("@sha256:") else {
        return Err(PlanError::InvalidImageReference(value.into()));
    };
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PlanError::InvalidImageReference(value.into()));
    }
    Ok(())
}

fn stable_hash(plan: &DeploymentPlan) -> Result<String, PlanError> {
    let mut copy = plan.clone();
    copy.plan_hash.clear();
    let bytes =
        serde_json::to_vec(&copy).map_err(|e| PlanError::InvalidImageReference(e.to_string()))?;
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("fnv1a-{hash:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::ProductBlueprint;
    use crate::registry::AppRegistry;

    fn registry() -> AppRegistry {
        AppRegistry::from_yaml(r#"
api_version: musical-lamp/v1
kind: AppRegistry
apps:
  - { id: frappe, name: Frappe Framework, kind: framework, source: { repository: https://github.com/frappe/frappe.git, ref: v16.9.0, resolved_commit: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa }, dependencies: [], compatibility: { framework_major: "16" }, installation: { order: 0 }, configuration: { fields: [] } }
  - { id: lms, name: LMS, kind: application, source: { repository: https://github.com/frappe/lms.git, ref: v1.0.0, resolved_commit: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb }, dependencies: [], compatibility: { framework: frappe, framework_major: "16" }, installation: { order: 10 }, configuration: { fields: [] } }
  - { id: trivia_sync, name: Trivia Sync, kind: application, source: { repository: https://github.com/bootstrapprx/trivia_sync.git, ref: 8febcdeb7f644d72a14d729105a77e08e7d4d380, resolved_commit: 8febcdeb7f644d72a14d729105a77e08e7d4d380 }, dependencies: [lms], compatibility: { framework: frappe, framework_major: "16" }, installation: { order: 20 }, configuration: { fields: [] } }
"#).unwrap()
    }
    fn blueprint(app: &str) -> ProductBlueprint {
        ProductBlueprint::from_yaml(&format!("api_version: musical-lamp/v1\nkind: ProductBlueprint\nmetadata: {{ id: test, name: Test, revision: 1 }}\nframework: {{ app: frappe }}\napps: [{{ app: {app} }}]\nsites: [{{ name: test.example.invalid }}]\nruntime: {{ image: {{ policy: pull, repository: registry.example/test, reference: registry.example/test@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa }}, database: {{ engine: mariadb, image: mariadb@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa }}, cache: {{ engine: redis, image: redis@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb }} }}\n")).unwrap()
    }
    #[test]
    fn creates_deterministic_plan_with_framework_and_app_sources() {
        let p = DeploymentPlanner::new(&registry())
            .plan(&blueprint("trivia_sync"))
            .unwrap();
        assert_eq!(p.framework.app, "frappe");
        assert_eq!(
            p.applications
                .iter()
                .map(|a| a.app.as_str())
                .collect::<Vec<_>>(),
            ["lms", "trivia_sync"]
        );
        assert_eq!(p.image_policy, ImagePlanPolicy::Pull);
        assert_eq!(
            p,
            DeploymentPlanner::new(&registry())
                .plan(&blueprint("trivia_sync"))
                .unwrap()
        );
        assert!(!p.plan_hash.is_empty());
    }
    #[test]
    fn blocks_missing_immutable_commit() {
        let mut r = registry();
        r.apps
            .iter_mut()
            .find(|a| a.id == "lms")
            .unwrap()
            .source
            .resolved_commit = None;
        assert!(
            matches!(DeploymentPlanner::new(&r).plan(&blueprint("lms")), Err(PlanError::SourceNotResolved { app }) if app == "lms")
        );
    }
    #[test]
    fn blocks_confirmed_framework_incompatibility() {
        let mut r = registry();
        r.apps
            .iter_mut()
            .find(|a| a.id == "lms")
            .unwrap()
            .compatibility = serde_yaml::from_str("{ framework: other }").unwrap();
        assert!(
            matches!(DeploymentPlanner::new(&r).plan(&blueprint("lms")), Err(PlanError::Incompatible { app, .. }) if app == "lms")
        );
    }
    #[test]
    fn unknown_compatibility_requires_explicit_override() {
        let mut r = registry();
        r.apps
            .iter_mut()
            .find(|a| a.id == "lms")
            .unwrap()
            .compatibility = serde_yaml::Value::Null;
        assert!(
            matches!(DeploymentPlanner::new(&r).plan(&blueprint("lms")), Err(PlanError::CompatibilityUnknown { app }) if app == "lms")
        );
        let p = DeploymentPlanner::new(&r)
            .with_compatibility_override("lms", "validated outside registry")
            .plan(&blueprint("lms"))
            .unwrap();
        assert!(p
            .compatibility
            .iter()
            .any(|c| c.status == CompatibilityStatus::Overridden));
    }
}
