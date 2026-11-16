use crate::registry::{AppDefinition, AppKind, AppRegistry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductBlueprint {
    pub api_version: String,
    pub kind: String,
    pub metadata: BlueprintMetadata,
    pub framework: FrameworkRequirement,
    #[serde(default)]
    pub apps: Vec<AppSelection>,
    pub sites: Vec<SiteDefinition>,
    pub runtime: RuntimeDefinition,
    #[serde(default)]
    pub network: serde_yaml::Value,
    #[serde(default)]
    pub secrets: Vec<SecretReference>,
    #[serde(default)]
    pub deployment: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlueprintMetadata {
    pub id: String,
    pub name: String,
    pub revision: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameworkRequirement {
    pub app: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSelection {
    pub app: String,
    #[serde(default)]
    pub configuration: serde_yaml::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteDefinition {
    pub name: String,
    #[serde(default)]
    pub host_name: Option<String>,
    #[serde(default)]
    pub configuration: serde_yaml::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeDefinition {
    pub image: ImageDefinition,
    pub database: InfrastructureImage,
    pub cache: InfrastructureImage,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageDefinition {
    pub policy: ImagePolicy,
    pub repository: String,
    #[serde(default)]
    pub reference: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImagePolicy {
    #[serde(rename = "build")]
    Build,
    #[serde(rename = "pull")]
    Pull,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InfrastructureImage {
    pub engine: String,
    pub image: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretReference {
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedComposition {
    pub framework: String,
    pub applications: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    Parse(String),
    InvalidHeader,
    MissingFramework(String),
    FrameworkIsInstallable(String),
    MissingApp(String),
    AppIsFramework(String),
    MissingDependency { app: String, dependency: String },
    DependencyCycle(String),
    DuplicateSelectedApp(String),
    NoSites,
}

impl ProductBlueprint {
    pub fn from_yaml(text: &str) -> Result<Self, CompositionError> {
        let blueprint: Self =
            serde_yaml::from_str(text).map_err(|e| CompositionError::Parse(e.to_string()))?;
        blueprint.validate_shape()?;
        Ok(blueprint)
    }
    pub fn resolve(&self, registry: &AppRegistry) -> Result<ResolvedComposition, CompositionError> {
        self.validate_shape()?;
        let framework = registry
            .apps
            .iter()
            .find(|a| a.id == self.framework.app)
            .ok_or_else(|| CompositionError::MissingFramework(self.framework.app.clone()))?;
        if framework.kind != AppKind::Framework {
            return Err(CompositionError::FrameworkIsInstallable(
                framework.id.clone(),
            ));
        }
        let mut selected = BTreeSet::new();
        for selection in &self.apps {
            if !selected.insert(selection.app.clone()) {
                return Err(CompositionError::DuplicateSelectedApp(
                    selection.app.clone(),
                ));
            }
            if registry.installable(&selection.app).is_none() {
                return Err(if registry.apps.iter().any(|a| a.id == selection.app) {
                    CompositionError::AppIsFramework(selection.app.clone())
                } else {
                    CompositionError::MissingApp(selection.app.clone())
                });
            }
        }
        let mut all = HashMap::new();
        for id in selected {
            visit_app(&id, registry, &mut all, &mut Vec::new())?;
        }
        let mut ordered: Vec<AppDefinition> = all.into_values().collect();
        ordered.sort_by_key(|a| (a.installation.order, a.id.clone()));
        Ok(ResolvedComposition {
            framework: framework.id.clone(),
            applications: ordered.into_iter().map(|a| a.id).collect(),
        })
    }
    fn validate_shape(&self) -> Result<(), CompositionError> {
        if self.api_version != "musical-lamp/v1" || self.kind != "ProductBlueprint" {
            return Err(CompositionError::InvalidHeader);
        }
        if self.sites.is_empty() {
            return Err(CompositionError::NoSites);
        }
        Ok(())
    }
}

fn visit_app(
    id: &str,
    registry: &AppRegistry,
    resolved: &mut HashMap<String, AppDefinition>,
    visiting: &mut Vec<String>,
) -> Result<(), CompositionError> {
    if resolved.contains_key(id) {
        return Ok(());
    }
    if visiting.iter().any(|current| current == id) {
        return Err(CompositionError::DependencyCycle(id.to_string()));
    }
    let app = registry
        .installable(id)
        .ok_or_else(|| CompositionError::MissingApp(id.to_string()))?
        .clone();
    visiting.push(id.to_string());
    for dependency in &app.dependencies {
        if registry
            .apps
            .iter()
            .all(|candidate| candidate.id != *dependency)
        {
            return Err(CompositionError::MissingDependency {
                app: id.to_string(),
                dependency: dependency.clone(),
            });
        }
        visit_app(dependency, registry, resolved, visiting)?;
    }
    visiting.pop();
    resolved.insert(id.to_string(), app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registry() -> AppRegistry {
        AppRegistry::from_yaml(r#"
api_version: musical-lamp/v1
kind: AppRegistry
apps:
  - { id: frappe, name: Frappe, kind: framework, source: { repository: https://github.com/frappe/frappe.git, ref: version-16 }, dependencies: [], compatibility: {}, installation: { order: 0 }, configuration: { fields: [] } }
  - { id: lms, name: LMS, kind: application, source: { repository: https://github.com/frappe/lms.git, ref: version-16 }, dependencies: [], compatibility: {}, installation: { order: 10 }, configuration: { fields: [] } }
  - { id: trivia_sync, name: Trivia Sync, kind: application, source: { repository: https://github.com/bootstrapprx/trivia_sync.git, ref: 8febcdeb7f644d72a14d729105a77e08e7d4d380 }, dependencies: [lms], compatibility: {}, installation: { order: 20 }, configuration: { fields: [] } }
"#).unwrap()
    }
    fn blueprint(apps: &str) -> String {
        format!("api_version: musical-lamp/v1\nkind: ProductBlueprint\nmetadata: {{ id: test, name: Test, revision: 1 }}\nframework: {{ app: frappe }}\napps: {apps}\nsites: [{{ name: test.example.invalid }}]\nruntime: {{ image: {{ policy: build, repository: example/test }}, database: {{ engine: mariadb, image: mariadb@sha256:abc }}, cache: {{ engine: redis, image: redis@sha256:def }} }}\n")
    }
    #[test]
    fn resolves_transitive_dependencies_and_keeps_framework_out_of_apps() {
        let b = ProductBlueprint::from_yaml(&blueprint("[{ app: trivia_sync }]")).unwrap();
        let r = b.resolve(&registry()).unwrap();
        assert_eq!(r.framework, "frappe");
        assert_eq!(r.applications, ["lms", "trivia_sync"]);
    }
    #[test]
    fn rejects_duplicate_selected_apps() {
        let b = ProductBlueprint::from_yaml(&blueprint("[{ app: lms }, { app: lms }]")).unwrap();
        assert!(
            matches!(b.resolve(&registry()), Err(CompositionError::DuplicateSelectedApp(id)) if id == "lms")
        );
    }
}
