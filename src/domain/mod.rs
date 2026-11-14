use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Product {
    Accounting,
    Trivia,
}

impl Product {
    pub fn label(self) -> &'static str {
        match self {
            Product::Accounting => "accounting",
            Product::Trivia => "trivia",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Product::Accounting => Product::Trivia,
            Product::Trivia => Product::Accounting,
        }
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
