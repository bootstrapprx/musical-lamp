use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRegistry {
    pub api_version: String,
    pub kind: String,
    pub apps: Vec<AppDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppDefinition {
    pub id: String,
    pub name: String,
    pub kind: AppKind,
    pub source: SourceDefinition,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub compatibility: serde_yaml::Value,
    pub installation: InstallationDefinition,
    #[serde(default)]
    pub configuration: ConfigurationDefinition,
    #[serde(default)]
    pub evidence: Vec<serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    Framework,
    Application,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceDefinition {
    pub repository: String,
    #[serde(rename = "ref")]
    pub source_ref: String,
    #[serde(default)]
    pub resolved_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallationDefinition {
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConfigurationDefinition {
    #[serde(default)]
    pub fields: Vec<ConfigurationField>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigurationField {
    pub key: String,
    pub classification: ConfigurationClassification,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "type")]
    pub value_type: Option<ConfigurationValueType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationClassification {
    Public,
    Sensitive,
    Derived,
    UserSupplied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationValueType {
    String,
    Integer,
    Boolean,
    Url,
    SecretRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Io(String),
    Parse(String),
    InvalidHeader,
    DuplicateApp(String),
    InvalidAppId(String),
    InvalidSourceRef { app: String, source_ref: String },
    FrameworkHasDependencies(String),
    MissingDependency { app: String, dependency: String },
}

impl AppRegistry {
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let text =
            fs::read_to_string(path).map_err(|error| RegistryError::Io(error.to_string()))?;
        Self::from_yaml(&text)
    }

    pub fn from_yaml(text: &str) -> Result<Self, RegistryError> {
        let registry: Self =
            serde_yaml::from_str(text).map_err(|error| RegistryError::Parse(error.to_string()))?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn validate(&self) -> Result<(), RegistryError> {
        if self.api_version != "musical-lamp/v1" || self.kind != "AppRegistry" {
            return Err(RegistryError::InvalidHeader);
        }

        let mut ids = HashSet::new();
        for app in &self.apps {
            if !is_valid_id(&app.id) {
                return Err(RegistryError::InvalidAppId(app.id.clone()));
            }
            if !ids.insert(app.id.clone()) {
                return Err(RegistryError::DuplicateApp(app.id.clone()));
            }
            if is_mutable_ref(&app.source.source_ref) {
                return Err(RegistryError::InvalidSourceRef {
                    app: app.id.clone(),
                    source_ref: app.source.source_ref.clone(),
                });
            }
            if app.kind == AppKind::Framework && !app.dependencies.is_empty() {
                return Err(RegistryError::FrameworkHasDependencies(app.id.clone()));
            }
        }

        for app in &self.apps {
            for dependency in &app.dependencies {
                if !ids.contains(dependency) {
                    return Err(RegistryError::MissingDependency {
                        app: app.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn frameworks(&self) -> impl Iterator<Item = &AppDefinition> {
        self.apps
            .iter()
            .filter(|app| app.kind == AppKind::Framework)
    }

    pub fn installable(&self, id: &str) -> Option<&AppDefinition> {
        self.apps
            .iter()
            .find(|app| app.id == id && app.kind == AppKind::Application)
    }
}

fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn is_mutable_ref(source_ref: &str) -> bool {
    matches!(source_ref, "main" | "master" | "develop" | "HEAD")
        || source_ref.starts_with("refs/heads/")
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = r#"
api_version: musical-lamp/v1
kind: AppRegistry
apps:
  - id: frappe
    name: Frappe Framework
    kind: framework
    source:
      repository: https://github.com/frappe/frappe.git
      ref: version-16
    dependencies: []
    compatibility: {}
    installation: { order: 0 }
    configuration: { fields: [] }
  - id: lms
    name: LMS
    kind: application
    source:
      repository: https://github.com/frappe/lms.git
      ref: version-16
    dependencies: []
    compatibility: {}
    installation: { order: 10 }
    configuration: { fields: [] }
"#;

    #[test]
    fn loads_registry_and_preserves_framework_boundary() {
        let registry = AppRegistry::from_yaml(REGISTRY).expect("registry should load");
        assert_eq!(registry.apps.len(), 2);
        assert_eq!(
            registry
                .frameworks()
                .map(|app| app.id.as_str())
                .collect::<Vec<_>>(),
            ["frappe"]
        );
        assert!(registry.installable("lms").is_some());
        assert!(registry.installable("frappe").is_none());
    }

    #[test]
    fn rejects_duplicate_ids_and_mutable_refs() {
        let duplicate = REGISTRY.replace("  - id: lms", "  - id: frappe");
        assert!(
            matches!(AppRegistry::from_yaml(&duplicate), Err(RegistryError::DuplicateApp(id)) if id == "frappe")
        );

        let mutable_ref = REGISTRY.replace("ref: version-16", "ref: main");
        assert!(matches!(
            AppRegistry::from_yaml(&mutable_ref),
            Err(RegistryError::InvalidSourceRef { .. })
        ));
    }

    #[test]
    fn loads_registry_from_file() {
        let dir =
            std::env::temp_dir().join(format!("musical-lamp-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("apps.yaml");
        std::fs::write(
            &path,
            "api_version: musical-lamp/v1\nkind: AppRegistry\napps: []\n",
        )
        .unwrap();
        let registry = AppRegistry::load(&path).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(registry.apps.is_empty());
    }
}
