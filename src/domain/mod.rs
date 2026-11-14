use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub region: String,
    pub host: String,
    pub ssh_user: String,
    pub provider: String,
    pub monthly_cost: f64,
    pub online: bool,
    pub cpu_pct: f64,
    pub ram_pct: f64,
    pub cpu_history: Vec<u64>,
    pub ram_history: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub node_id: String,
    pub stack: String,
    pub cpu_share_pct: f64,
    pub ram_share_pct: f64,
    pub status: String,
    pub logs: Vec<String>,
}

impl Node {
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.region)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Product {
    id: String,
}

impl Product {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    pub fn label(&self) -> &str {
        &self.id
    }

    pub fn is_valid_id(id: &str) -> bool {
        !id.is_empty()
            && id != "."
            && id != ".."
            && !id.contains('/')
            && !id.contains('\\')
            && !id.chars().any(|c| c.is_control())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProductStatus {
    Ready,
    NoEnv,
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductEntry {
    pub product: Product,
    pub status: ProductStatus,
}

pub fn discover_products() -> Vec<Product> {
    discover_product_entries()
        .into_iter()
        .map(|e| e.product)
        .collect()
}

pub fn discover_product_entries() -> Vec<ProductEntry> {
    let root = Path::new("products");
    let mut entries: Vec<ProductEntry> = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                return None;
            };
            if !file_type.is_dir() {
                return None;
            }
            let raw_name = entry.file_name();
            let name = raw_name.to_string_lossy().to_string();
            let has_marker = ["apps.json", "build.env", ".env.example", ".env"]
                .iter()
                .any(|name| path.join(name).is_file());
            if !has_marker {
                return None;
            }
            if !Product::is_valid_id(&name) {
                return Some(ProductEntry {
                    product: Product::new(name.clone()),
                    status: ProductStatus::Invalid("nome de diretório inválido/inseguro".into()),
                });
            }
            let status = if path.join(".env").is_file() || path.join(".env").is_symlink() {
                ProductStatus::Ready
            } else if path.join(".env.example").is_file() {
                ProductStatus::NoEnv
            } else {
                ProductStatus::Ready
            };
            Some(ProductEntry {
                product: Product::new(name),
                status,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.product.label().cmp(b.product.label()));
    entries
}

pub fn other_product(product: &Product) -> Option<Product> {
    discover_products()
        .into_iter()
        .find(|candidate| candidate != product)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_invalid_product_ids() {
        assert!(!Product::is_valid_id(""));
        assert!(!Product::is_valid_id("."));
        assert!(!Product::is_valid_id(".."));
        assert!(!Product::is_valid_id("a/b"));
        assert!(!Product::is_valid_id("a\\b"));
        assert!(Product::is_valid_id("demo-product_1"));
    }

    #[test]
    fn discovers_products_deterministically_in_temp_dir() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("musical-lamp-domain-{nanos}"));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("products/beta")).unwrap();
        fs::create_dir_all(base.join("products/alpha")).unwrap();
        fs::create_dir_all(base.join("products/invalid/name")).unwrap();
        fs::write(base.join("products/beta/.env.example"), "A=1\n").unwrap();
        fs::write(base.join("products/alpha/apps.json"), "[]\n").unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&base).unwrap();
        let products = discover_products();
        std::env::set_current_dir(prev).unwrap();
        let _ = fs::remove_dir_all(&base);
        assert_eq!(
            products.iter().map(|p| p.label()).collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductEnv {
    pub product: Product,
    pub path_hint: String,
    pub vars: Vec<EnvVar>,
    pub dirty: bool,
    pub last_action: String,
    pub reveal_secrets: bool,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackState {
    Stopped,
    Starting,
    Running,
    Degraded,
    Stopping,
}

impl StackState {
    pub fn label(self) -> &'static str {
        match self {
            StackState::Stopped => "stopped",
            StackState::Starting => "starting",
            StackState::Running => "running",
            StackState::Degraded => "degraded",
            StackState::Stopping => "stopping",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsStatus {
    pub product: Product,
    pub state: StackState,
    pub last_action: String,
    pub log_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreflightStatus {
    Pending,
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheck {
    pub name: String,
    pub status: PreflightStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoctorFixKind {
    CreateEnvFromExample,
    AddMissingEnvKeys,
    FillSecret,
    FixEnvPermissions,
    SaveEnvNormalized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorIssue {
    pub severity: PreflightStatus,
    pub title: String,
    pub detail: String,
    pub fix: Option<DoctorFixKind>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoctorReport {
    pub issues: Vec<DoctorIssue>,
    pub applied: Vec<String>,
    pub failed: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingStep {
    pub title: String,
    pub done: bool,
    pub detail: String,
}

pub fn allocate_costs(nodes: &[Node], containers: &[Container]) -> Vec<(String, f64)> {
    // returns (container_id or special "__idle__:<node_id>", monthly_cost)
    let mut out = Vec::new();
    for node in nodes {
        let node_containers: Vec<&Container> =
            containers.iter().filter(|c| c.node_id == node.id).collect();
        let mut allocated = 0.0;
        for c in &node_containers {
            let share = (c.cpu_share_pct * 0.5 + c.ram_share_pct * 0.5).clamp(0.0, 100.0) / 100.0;
            let cost = node.monthly_cost * share;
            allocated += cost;
            out.push((c.id.clone(), cost));
        }
        let idle = (node.monthly_cost - allocated).max(0.0);
        out.push((format!("__idle__:{}", node.id), idle));
    }
    out
}
