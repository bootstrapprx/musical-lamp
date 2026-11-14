use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::{
    allocate_costs, Container, DoctorFixKind, DoctorIssue, DoctorReport, EnvVar, Node,
    OnboardingStep, OpsStatus, PreflightCheck, PreflightStatus, Product, ProductEnv, StackState,
};
use crate::event::{Action, Event};
use crate::ops_core::{envfile, secrets, validate};
use crate::orchestration;
use crate::telemetry::TelemetryProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    ClusterFinOps,
    ComposeStacks,
    BridgesPlugins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Nodes,
    Containers,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Logs,
    CostBreakdown,
    ComposeYaml,
    Forecast7d,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackPanel {
    Overview,
    Operations,
    AppRepos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgePanel {
    Env,
    Tunnel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    None,
    AddNode(AddNodeForm),
    DropNode { confirm: bool },
    AddApp(AddAppForm),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    Name,
    Host,
    User,
    Provider,
    Cost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddNodeForm {
    pub name: String,
    pub host: String,
    pub user: String,
    pub provider: String,
    pub cost: String,
    pub field: AddField,
}

impl Default for AddNodeForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            user: String::from("root"),
            provider: String::from("Hetzner"),
            cost: String::from("10.00"),
            field: AddField::Name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppField {
    Url,
    Branch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddAppForm {
    pub url: String,
    pub branch: String,
    pub field: AppField,
}

impl Default for AddAppForm {
    fn default() -> Self {
        Self {
            url: String::new(),
            branch: String::new(),
            field: AppField::Url,
        }
    }
}

pub struct AppState {
    pub should_quit: bool,
    pub view: View,
    pub focus: FocusPane,
    pub detail_tab: DetailTab,
    pub stack_panel: StackPanel,
    pub bridge_panel: BridgePanel,
    pub modal: Modal,
    pub nodes: Vec<Node>,
    pub containers: Vec<Container>,
    pub selected_node: usize,
    pub selected_container: usize,
    pub budget: f64,
    pub telemetry: Box<dyn TelemetryProvider + Send>,
    pub log_scroll: u16,
    pub featured_product: Product,
    pub env_states: HashMap<Product, ProductEnv>,
    pub env_selected: usize,
    pub env_editing: bool,
    pub env_input: String,
    pub ops_states: HashMap<Product, OpsStatus>,
    pub ops_selected_action: usize,
    pub preflight: Vec<PreflightCheck>,
    pub preflight_ran: bool,
    pub onboarding_steps: Vec<OnboardingStep>,
    pub show_onboarding: bool,
    pub apps_selected: usize,
    pub doctor: crate::domain::DoctorReport,
    pub doctor_selected: usize,
}

impl AppState {
    pub fn new(telemetry: Box<dyn TelemetryProvider + Send>) -> Self {
        let nodes = vec![
            Node {
                id: "n1".into(),
                name: "hetzner-hel1".into(),
                region: "hel1".into(),
                host: "10.0.0.11".into(),
                ssh_user: "root".into(),
                provider: "Hetzner".into(),
                monthly_cost: 13.99,
                online: true,
                cpu_pct: 34.0,
                ram_pct: 48.0,
                cpu_history: vec![30, 32, 34],
                ram_history: vec![45, 46, 48],
            },
            Node {
                id: "n2".into(),
                name: "aws-use1".into(),
                region: "us-east-1".into(),
                host: "10.0.0.21".into(),
                ssh_user: "ubuntu".into(),
                provider: "AWS".into(),
                monthly_cost: 24.50,
                online: true,
                cpu_pct: 52.0,
                ram_pct: 61.0,
                cpu_history: vec![50, 51, 52],
                ram_history: vec![58, 60, 61],
            },
            Node {
                id: "n3".into(),
                name: "do-nyc3".into(),
                region: "nyc3".into(),
                host: "10.0.0.31".into(),
                ssh_user: "root".into(),
                provider: "DigitalOcean".into(),
                monthly_cost: 12.00,
                online: false,
                cpu_pct: 0.0,
                ram_pct: 0.0,
                cpu_history: vec![0, 0, 0],
                ram_history: vec![0, 0, 0],
            },
        ];
        let containers = vec![
            Container {
                id: "c1".into(),
                name: "web-frontend".into(),
                node_id: "n1".into(),
                stack: "trivia".into(),
                cpu_share_pct: 12.0,
                ram_share_pct: 18.0,
                status: "running".into(),
                logs: vec!["[boot] frontend ready".into()],
            },
            Container {
                id: "c2".into(),
                name: "api-backend".into(),
                node_id: "n1".into(),
                stack: "trivia".into(),
                cpu_share_pct: 14.0,
                ram_share_pct: 20.0,
                status: "running".into(),
                logs: vec!["[boot] api listening".into()],
            },
            Container {
                id: "c3".into(),
                name: "worker-jobs".into(),
                node_id: "n1".into(),
                stack: "trivia".into(),
                cpu_share_pct: 6.0,
                ram_share_pct: 8.0,
                status: "running".into(),
                logs: vec!["[boot] worker started".into()],
            },
            Container {
                id: "c4".into(),
                name: "accounting-web".into(),
                node_id: "n2".into(),
                stack: "accounting".into(),
                cpu_share_pct: 20.0,
                ram_share_pct: 24.0,
                status: "running".into(),
                logs: vec!["[boot] accounting ui".into()],
            },
            Container {
                id: "c5".into(),
                name: "postgres".into(),
                node_id: "n2".into(),
                stack: "accounting".into(),
                cpu_share_pct: 18.0,
                ram_share_pct: 26.0,
                status: "running".into(),
                logs: vec!["[boot] db ready".into()],
            },
            Container {
                id: "c6".into(),
                name: "redis".into(),
                node_id: "n2".into(),
                stack: "accounting".into(),
                cpu_share_pct: 8.0,
                ram_share_pct: 10.0,
                status: "running".into(),
                logs: vec!["[boot] cache warm".into()],
            },
        ];
        let mut env_states = HashMap::new();
        env_states.insert(Product::Accounting, load_env_state(Product::Accounting));
        env_states.insert(Product::Trivia, load_env_state(Product::Trivia));
        let mut ops_states = HashMap::new();
        for p in [Product::Accounting, Product::Trivia] {
            ops_states.insert(
                p,
                OpsStatus {
                    product: p,
                    state: StackState::Stopped,
                    last_action: "estado inicial: use Consultar para verificar a stack real".into(),
                    log_lines: vec![format!(
                        "[{}] pronto para executar ações reais (common/dc.sh {})",
                        p.label(),
                        p.label()
                    )],
                },
            );
        }

        let mut s = Self {
            should_quit: false,
            view: View::ClusterFinOps,
            focus: FocusPane::Nodes,
            detail_tab: DetailTab::Logs,
            stack_panel: StackPanel::Overview,
            bridge_panel: BridgePanel::Env,
            modal: Modal::None,
            nodes,
            containers,
            selected_node: 0,
            selected_container: 0,
            budget: 100.0,
            telemetry,
            log_scroll: 0,
            featured_product: Product::Accounting,
            env_states,
            env_selected: 0,
            env_editing: false,
            env_input: String::new(),
            ops_states,
            ops_selected_action: 0,
            preflight: Vec::new(),
            preflight_ran: false,
            onboarding_steps: Vec::new(),
            show_onboarding: false,
            apps_selected: 0,
            doctor: crate::domain::DoctorReport::default(),
            doctor_selected: 0,
        };
        s.refresh_onboarding();
        s.run_doctor_scan(false);
        s
    }

    pub fn projected_total(&self) -> f64 {
        self.nodes.iter().map(|n| n.monthly_cost).sum()
    }
    pub fn selected_node(&self) -> Option<&Node> {
        self.nodes.get(self.selected_node)
    }
    pub fn containers_for_selected_node(&self) -> Vec<&Container> {
        if let Some(n) = self.selected_node() {
            self.containers
                .iter()
                .filter(|c| c.node_id == n.id)
                .collect()
        } else {
            vec![]
        }
    }
    pub fn selected_container_obj(&self) -> Option<&Container> {
        let list = self.containers_for_selected_node();
        list.get(self.selected_container).copied()
    }
    pub fn container_costs_map(&self) -> HashMap<String, f64> {
        allocate_costs(&self.nodes, &self.containers)
            .into_iter()
            .collect()
    }
    pub fn current_env(&self) -> Option<&ProductEnv> {
        self.env_states.get(&self.featured_product)
    }
    pub fn current_env_mut(&mut self) -> Option<&mut ProductEnv> {
        self.env_states.get_mut(&self.featured_product)
    }
    pub fn current_ops(&self) -> Option<&OpsStatus> {
        self.ops_states.get(&self.featured_product)
    }

    pub fn handle_event(&mut self, event: Event) -> Option<Action> {
        match event {
            Event::Tick => {
                self.telemetry.tick(&mut self.nodes, &mut self.containers);
                None
            }
            Event::Resize => None,
            Event::Key(key) => self.handle_key(key),
            Event::OpsLine { product, line } => {
                self.push_ops_log(product, line);
                None
            }
            Event::OpsFinished {
                product,
                action,
                success,
                summary,
            } => {
                self.apply_ops_finished(product, &action, success, summary);
                None
            }
        }
    }

    fn push_ops_log(&mut self, product: Product, line: String) {
        if let Some(ops) = self.ops_states.get_mut(&product) {
            ops.log_lines.push(line);
            if ops.log_lines.len() > 300 {
                let drain = ops.log_lines.len() - 300;
                ops.log_lines.drain(0..drain);
            }
        }
    }

    fn apply_ops_finished(
        &mut self,
        product: Product,
        action: &str,
        success: bool,
        summary: String,
    ) {
        if let Some(ops) = self.ops_states.get_mut(&product) {
            ops.last_action = format!("{action}: {summary}");
            ops.log_lines.push(format!("[resultado] {summary}"));
            match action {
                "up" => {
                    ops.state = if success {
                        StackState::Running
                    } else {
                        StackState::Degraded
                    };
                }
                "down" => {
                    ops.state = if success {
                        StackState::Stopped
                    } else {
                        StackState::Degraded
                    };
                }
                "migrate" if !success => {
                    ops.state = StackState::Degraded;
                }
                _ => {}
            }
        }
        self.refresh_onboarding();
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match &self.modal {
            Modal::AddNode(_) => return self.handle_add_node_key(key),
            Modal::DropNode { .. } => return self.handle_drop_node_key(key),
            Modal::AddApp(_) => return self.handle_add_app_key(key),
            Modal::None => {}
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        if self.view == View::BridgesPlugins
            && self.bridge_panel == BridgePanel::Env
            && self.env_editing
        {
            return self.handle_env_edit_key(key);
        }

        match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('1') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::Logs;
                } else {
                    self.view = View::ClusterFinOps;
                }
                None
            }
            KeyCode::Char('2') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::CostBreakdown;
                } else {
                    self.view = View::ComposeStacks;
                }
                None
            }
            KeyCode::Char('3') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::ComposeYaml;
                } else {
                    self.view = View::BridgesPlugins;
                }
                None
            }
            KeyCode::Char('4')
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail =>
            {
                self.detail_tab = DetailTab::Forecast7d;
                None
            }
            KeyCode::Tab => {
                if self.view == View::ClusterFinOps {
                    self.focus = match self.focus {
                        FocusPane::Nodes => FocusPane::Containers,
                        FocusPane::Containers => FocusPane::Detail,
                        FocusPane::Detail => FocusPane::Nodes,
                    };
                } else if self.view == View::ComposeStacks {
                    self.stack_panel = match self.stack_panel {
                        StackPanel::Overview => StackPanel::Operations,
                        StackPanel::Operations => StackPanel::AppRepos,
                        StackPanel::AppRepos => StackPanel::Overview,
                    };
                } else if self.view == View::BridgesPlugins {
                    self.bridge_panel = match self.bridge_panel {
                        BridgePanel::Env => BridgePanel::Tunnel,
                        BridgePanel::Tunnel => BridgePanel::Env,
                    };
                }
                None
            }
            KeyCode::Char('p') => {
                self.featured_product = self.featured_product.toggle();
                self.clamp_env_selection();
                self.preflight.clear();
                self.preflight_ran = false;
                self.refresh_onboarding();
                self.apps_selected = 0;
                None
            }
            KeyCode::Up => {
                self.move_up();
                None
            }
            KeyCode::Down => {
                self.move_down();
                None
            }
            KeyCode::Char('a') if self.view == View::ClusterFinOps => {
                self.modal = Modal::AddNode(AddNodeForm::default());
                None
            }
            KeyCode::Char('d') if self.view == View::ClusterFinOps => {
                if !self.nodes.is_empty() {
                    self.modal = Modal::DropNode { confirm: false };
                }
                None
            }
            KeyCode::Char('s') if self.view == View::ClusterFinOps => {
                self.simulate_load();
                None
            }
            // Compose Stacks operations / app repos
            KeyCode::Char('u')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("up")
            }
            KeyCode::Char('x')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("down")
            }
            KeyCode::Char('c')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("status")
            }
            KeyCode::Char('f')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("preflight")
            }
            KeyCode::Char('i')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("onboarding")
            }
            KeyCode::Char('m')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("migrate")
            }
            KeyCode::Char('o')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("logs")
            }
            KeyCode::Char('k')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("doctor")
            }
            KeyCode::Char('K')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("doctor_fix")
            }
            KeyCode::Enter
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_selected_ops_action()
            }
            KeyCode::Char('a')
                if self.view == View::ComposeStacks && self.stack_panel == StackPanel::AppRepos =>
            {
                self.modal = Modal::AddApp(AddAppForm::default());
                None
            }
            KeyCode::Char('d')
                if self.view == View::ComposeStacks && self.stack_panel == StackPanel::AppRepos =>
            {
                self.remove_selected_app();
                None
            }
            // Bridges & Plugins env actions
            KeyCode::Enter
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.start_env_edit();
                None
            }
            KeyCode::Char('g')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.generate_env_from_example();
                None
            }
            KeyCode::Char('r')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.reload_env();
                None
            }
            KeyCode::Char('w')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.save_env();
                None
            }
            KeyCode::Char('v')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.toggle_env_reveal();
                None
            }
            KeyCode::Char('n')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.generate_secret_for_selected();
                None
            }
            _ => None,
        }
    }

    fn move_up(&mut self) {
        match self.view {
            View::ClusterFinOps => match self.focus {
                FocusPane::Nodes => {
                    if self.selected_node > 0 {
                        self.selected_node -= 1;
                        self.selected_container = 0;
                    }
                }
                FocusPane::Containers => {
                    self.selected_container = self.selected_container.saturating_sub(1);
                }
                FocusPane::Detail => {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                }
            },
            View::ComposeStacks => match self.stack_panel {
                StackPanel::Operations => {
                    self.ops_selected_action = self.ops_selected_action.saturating_sub(1);
                }
                StackPanel::AppRepos => {
                    self.apps_selected = self.apps_selected.saturating_sub(1);
                }
                StackPanel::Overview => {}
            },
            View::BridgesPlugins => {
                if self.bridge_panel == BridgePanel::Env && self.env_selected > 0 {
                    self.env_selected -= 1;
                }
            }
        }
    }
    fn move_down(&mut self) {
        match self.view {
            View::ClusterFinOps => match self.focus {
                FocusPane::Nodes => {
                    if self.selected_node + 1 < self.nodes.len() {
                        self.selected_node += 1;
                        self.selected_container = 0;
                    }
                }
                FocusPane::Containers => {
                    let len = self.containers_for_selected_node().len().saturating_add(1);
                    if self.selected_container + 1 < len {
                        self.selected_container += 1;
                    }
                }
                FocusPane::Detail => {
                    self.log_scroll = self.log_scroll.saturating_add(1);
                }
            },
            View::ComposeStacks => match self.stack_panel {
                StackPanel::Operations => {
                    if self.ops_selected_action < 7 {
                        self.ops_selected_action += 1;
                    }
                }
                StackPanel::AppRepos => {
                    let len = self.apps_for_featured().len();
                    if len > 0 && self.apps_selected + 1 < len {
                        self.apps_selected += 1;
                    }
                }
                StackPanel::Overview => {}
            },
            View::BridgesPlugins => {
                if self.bridge_panel == BridgePanel::Env {
                    if let Some(env) = self.current_env() {
                        if self.env_selected + 1 < env.vars.len() {
                            self.env_selected += 1;
                        }
                    }
                }
            }
        }
    }

    fn simulate_load(&mut self) {
        if let Some(n) = self.nodes.get_mut(self.selected_node) {
            if n.online {
                n.cpu_pct = (n.cpu_pct + 15.0).min(99.0);
                n.ram_pct = (n.ram_pct + 10.0).min(99.0);
            }
        }
    }

    fn clamp_env_selection(&mut self) {
        if let Some(env) = self.current_env() {
            if env.vars.is_empty() {
                self.env_selected = 0;
            } else if self.env_selected >= env.vars.len() {
                self.env_selected = env.vars.len() - 1;
            }
        }
    }

    fn start_env_edit(&mut self) {
        let val = self
            .current_env()
            .and_then(|e| e.vars.get(self.env_selected).map(|v| v.value.clone()))
            .unwrap_or_default();
        self.env_editing = true;
        self.env_input = val;
    }

    fn handle_env_edit_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => {
                self.env_editing = false;
                self.env_input.clear();
            }
            KeyCode::Enter => {
                let idx = self.env_selected;
                let input = self.env_input.clone();
                if let Some(env) = self.current_env_mut() {
                    if let Some(var) = env.vars.get_mut(idx) {
                        var.value = input;
                        env.dirty = true;
                        env.last_action = "valor atualizado (não salvo)".into();
                    }
                }
                self.env_editing = false;
                self.env_input.clear();
            }
            KeyCode::Backspace => {
                self.env_input.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.env_input.push(c);
            }
            _ => {}
        }
        None
    }

    fn generate_env_from_example(&mut self) {
        let product = self.featured_product;
        let example = product_env_example_path(product);
        let target = product_env_path(product);
        match fs::read_to_string(&example) {
            Ok(content) => {
                let vars = parse_env_vars(&content);
                if let Some(env) = self.env_states.get_mut(&product) {
                    env.vars = vars;
                    env.dirty = true;
                    env.last_action = format!("gerado a partir de {} (não salvo)", example);
                    env.path_hint = target;
                }
            }
            Err(e) => {
                if let Some(env) = self.env_states.get_mut(&product) {
                    env.last_action = format!("falha ao ler exemplo: {e}");
                }
            }
        }
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn toggle_env_reveal(&mut self) {
        if let Some(env) = self.current_env_mut() {
            env.reveal_secrets = !env.reveal_secrets;
            env.last_action = if env.reveal_secrets {
                "segredos visíveis".into()
            } else {
                "segredos mascarados".into()
            };
        }
    }

    fn generate_secret_for_selected(&mut self) {
        let idx = self.env_selected;
        let Some(env) = self.current_env_mut() else {
            return;
        };
        let Some(var_key) = env.vars.get(idx).map(|v| v.key.clone()) else {
            return;
        };
        let kind = secrets::default_kind_for(&var_key).unwrap_or(secrets::SecretKind::Hex(24));
        match secrets::generate(kind) {
            Ok(v) => {
                if let Some(var) = env.vars.get_mut(idx) {
                    var.value = v;
                }
                env.dirty = true;
                env.last_action = secrets::rotation_note(&var_key)
                    .map(|n| format!("segredo gerado: {n}"))
                    .unwrap_or_else(|| "segredo gerado (não salvo)".into());
            }
            Err(e) => env.last_action = format!("falha ao gerar segredo: {e}"),
        }
    }

    fn reload_env(&mut self) {
        let product = self.featured_product;
        self.env_states.insert(product, load_env_state(product));
        self.clamp_env_selection();
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn save_env(&mut self) {
        let product = self.featured_product;
        let path = product_env_path(product);
        let content = if let Some(env) = self.current_env() {
            render_env_file(&env.vars)
        } else {
            return;
        };
        let result = fs::write(&path, content);
        if let Some(env) = self.current_env_mut() {
            match result {
                Ok(_) => {
                    env.dirty = false;
                    env.last_action = format!("salvo em {path}");
                }
                Err(e) => {
                    env.last_action = format!("erro ao salvar: {e}");
                }
            }
        }
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn run_ops_action(&mut self, action: &str) -> Option<Action> {
        let product = self.featured_product;
        match action {
            "preflight" => {
                self.run_preflight();
                self.run_doctor_scan(false);
                return None;
            }
            "doctor" => {
                self.run_doctor_scan(false);
                return None;
            }
            "doctor_fix" => {
                self.run_doctor_scan(true);
                return None;
            }
            "onboarding" => {
                self.show_onboarding = !self.show_onboarding;
                self.refresh_onboarding();
                if let Some(ops) = self.ops_states.get_mut(&product) {
                    ops.last_action = if self.show_onboarding {
                        "onboarding aberto".into()
                    } else {
                        "onboarding fechado".into()
                    };
                }
                return None;
            }
            _ => {}
        }

        if let Some(cmd_action) = orchestration::build_ops_action(product, action) {
            if let Some(ops) = self.ops_states.get_mut(&product) {
                match action {
                    "up" => {
                        ops.state = StackState::Starting;
                        ops.last_action = "subindo a stack (execução real em andamento)".into();
                    }
                    "down" => {
                        ops.state = StackState::Stopping;
                        ops.last_action = "parando a stack (execução real em andamento)".into();
                    }
                    "status" => {
                        ops.last_action = "consultando estado real da stack".into();
                    }
                    "logs" => {
                        ops.last_action = "consultando logs reais da stack".into();
                    }
                    "migrate" => {
                        ops.last_action =
                            "executando migração real (bench --site all migrate)".into();
                    }
                    _ => {}
                }
            }
            return Some(cmd_action);
        }
        None
    }

    fn run_selected_ops_action(&mut self) -> Option<Action> {
        let action = match self.ops_selected_action {
            0 => "up",
            1 => "down",
            2 => "status",
            3 => "preflight",
            4 => "doctor",
            5 => "onboarding",
            6 => "migrate",
            _ => "logs",
        };
        self.run_ops_action(action)
    }

    pub fn run_doctor_scan(&mut self, apply_fixes: bool) {
        let product = self.featured_product;
        let label = product.label();
        let env_path_s = product_env_path(product);
        let example_path_s = product_env_example_path(product);
        let env_path = Path::new(&env_path_s);
        let example_path = Path::new(&example_path_s);
        let prod_dir = Path::new("products").join(label);

        let mut report = DoctorReport {
            issues: Vec::new(),
            applied: Vec::new(),
            summary: String::new(),
        };

        let env_exists = env_path.exists();
        let example_exists = example_path.exists();

        if !env_exists {
            report.issues.push(DoctorIssue {
                severity: PreflightStatus::Error,
                title: "Arquivo .env ausente".into(),
                detail: format!(
                    "products/{label}/.env não existe; a stack não pode subir sem configuração."
                ),
                fix: if example_exists {
                    Some(DoctorFixKind::CreateEnvFromExample)
                } else {
                    None
                },
                key: None,
            });
        }

        // Build a working env view for analysis (current in-memory or file/example)
        let mut working_vars: Vec<EnvVar> = if let Some(env) = self.env_states.get(&product) {
            env.vars.clone()
        } else {
            Vec::new()
        };
        if working_vars.is_empty() {
            if env_exists {
                if let Ok(c) = fs::read_to_string(env_path) {
                    working_vars = parse_env_vars(&c);
                }
            } else if example_exists {
                if let Ok(c) = fs::read_to_string(example_path) {
                    working_vars = parse_env_vars(&c);
                }
            }
        }

        // Drift: missing keys from example
        if example_exists {
            if let Ok(ex_c) = fs::read_to_string(example_path) {
                let ex_vars = parse_env_vars(&ex_c);
                let ex_keys: std::collections::HashSet<String> =
                    ex_vars.iter().map(|v| v.key.clone()).collect();
                let cur_keys: std::collections::HashSet<String> =
                    working_vars.iter().map(|v| v.key.clone()).collect();
                let missing: Vec<String> = ex_vars
                    .iter()
                    .filter(|v| !cur_keys.contains(&v.key))
                    .map(|v| v.key.clone())
                    .collect();
                if !missing.is_empty() {
                    report.issues.push(DoctorIssue {
                        severity: PreflightStatus::Warn,
                        title: "Drift: chaves faltando no .env".into(),
                        detail: format!(
                            "Faltam {} chave(s) presentes no .env.example: {}",
                            missing.len(),
                            missing
                                .iter()
                                .take(6)
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        fix: Some(DoctorFixKind::AddMissingEnvKeys),
                        key: None,
                    });
                }
                let extra: Vec<String> = working_vars
                    .iter()
                    .filter(|v| !ex_keys.contains(&v.key))
                    .map(|v| v.key.clone())
                    .collect();
                if !extra.is_empty() {
                    report.issues.push(DoctorIssue {
                        severity: PreflightStatus::Warn,
                        title: "Drift: chaves locais/obsoletas".into(),
                        detail: format!("{} chave(s) não existem no .env.example (revise se são necessárias): {}", extra.len(), extra.iter().take(6).cloned().collect::<Vec<_>>().join(", ")),
                        fix: None,
                        key: None,
                    });
                }
            }
        }

        // Required/blank secrets and blocking empties
        let required = [
            "IMAGE_REF",
            "DB_PASSWORD",
            "TUNNEL_TOKEN",
            "EDGE_SUBNET",
            "EDGE_IP_RANGE",
            "CLOUDFLARED_IP",
        ];
        for req in required {
            let val = working_vars
                .iter()
                .find(|v| v.key == req)
                .map(|v| v.value.trim().to_string())
                .unwrap_or_default();
            if val.is_empty() {
                let can_fix = matches!(req, "DB_PASSWORD") || secrets::can_auto_generate(req);
                let (sev, note) = match req {
                    "DB_PASSWORD" => (PreflightStatus::Error, "senha do banco vazia; pode ser gerada automaticamente."),
                    "TUNNEL_TOKEN" => (PreflightStatus::Error, "token do túnel vazio; informe o token do Cloudflare Zero Trust (valor automático serve só como placeholder local)."),
                    "IMAGE_REF" => (PreflightStatus::Error, "referência da imagem vazia; defina uma tag/digest fixa do produto."),
                    _ => (PreflightStatus::Error, "valor obrigatório vazio."),
                };
                report.issues.push(DoctorIssue {
                    severity: sev,
                    title: format!("{req} vazio"),
                    detail: note.into(),
                    fix: if can_fix {
                        Some(DoctorFixKind::FillSecret)
                    } else {
                        None
                    },
                    key: Some(req.into()),
                });
            }
        }

        // Validation findings from ops_core::validate
        if env_exists || !working_vars.is_empty() {
            let content = render_env_file(&working_vars);
            let ef = envfile::EnvFile::parse(&content);
            let other = fs::read_to_string(product_env_path(product.toggle()))
                .ok()
                .map(|c| envfile::EnvFile::parse(&c));
            let findings = validate::validate_env(&ef, env_path, other.as_ref());
            for fnd in findings {
                let sev = match fnd.severity {
                    validate::Severity::Error => PreflightStatus::Error,
                    validate::Severity::Warn => PreflightStatus::Warn,
                    validate::Severity::Info => PreflightStatus::Pending,
                };
                let fix = match (fnd.key.as_deref(), fnd.severity) {
                    (None, validate::Severity::Warn) if fnd.message.contains("chmod 600") => {
                        Some(DoctorFixKind::FixEnvPermissions)
                    }
                    _ => None,
                };
                // avoid duplicating required-empty issues too noisily
                if fnd
                    .key
                    .as_deref()
                    .map(|k| required.contains(&k) && ef.is_blank(k))
                    .unwrap_or(false)
                {
                    continue;
                }
                report.issues.push(DoctorIssue {
                    severity: sev,
                    title: fnd
                        .key
                        .clone()
                        .unwrap_or_else(|| "Validação do ambiente".into()),
                    detail: fnd.message,
                    fix,
                    key: fnd.key,
                });
            }
            if example_exists {
                if let Ok(ex_c) = fs::read_to_string(example_path) {
                    let exf = envfile::EnvFile::parse(&ex_c);
                    for fnd in validate::drift(&ef, &exf) {
                        // drift additions/removals already summarized above; keep other infos only
                        if matches!(fnd.severity, validate::Severity::Info) {
                            continue;
                        }
                    }
                }
            }
        }

        // Build validation
        for fnd in validate::validate_build(&prod_dir) {
            let sev = match fnd.severity {
                validate::Severity::Error => PreflightStatus::Error,
                validate::Severity::Warn => PreflightStatus::Warn,
                validate::Severity::Info => PreflightStatus::Pending,
            };
            report.issues.push(DoctorIssue {
                severity: sev,
                title: "Build do produto".into(),
                detail: fnd.message,
                fix: None,
                key: fnd.key,
            });
        }

        // Deduplicate by (title, detail)
        let mut seen = std::collections::HashSet::new();
        report
            .issues
            .retain(|i| seen.insert((i.title.clone(), i.detail.clone())));

        // Apply fixes if requested
        if apply_fixes {
            #[cfg(unix)]
            fn restrict_env_permissions(path: &Path) -> Result<(), String> {
                use std::os::unix::fs::PermissionsExt;
                let meta = fs::metadata(path).map_err(|e| e.to_string())?;
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                fs::set_permissions(path, perms).map_err(|e| e.to_string())
            }

            let mut changed = false;
            // 1) create env from example
            if !env_exists
                && example_exists
                && report
                    .issues
                    .iter()
                    .any(|i| matches!(i.fix, Some(DoctorFixKind::CreateEnvFromExample)))
            {
                if let Ok(content) = fs::read_to_string(example_path) {
                    if let Some(parent) = env_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    match fs::write(env_path, &content) {
                        Ok(_) => {
                            report.applied.push(format!(
                                "Criado products/{label}/.env a partir do .env.example"
                            ));
                            #[cfg(unix)]
                            match restrict_env_permissions(env_path) {
                                Ok(_) => report
                                    .applied
                                    .push("Permissões do .env ajustadas para 600".into()),
                                Err(e) => report
                                    .applied
                                    .push(format!("Falha ao ajustar permissões do .env: {e}")),
                            }
                            changed = true;
                        }
                        Err(e) => report.applied.push(format!("Falha ao criar .env: {e}")),
                    }
                }
            }
            // reload state after possible creation
            if changed {
                self.env_states.insert(product, load_env_state(product));
                working_vars = self
                    .current_env()
                    .map(|e| e.vars.clone())
                    .unwrap_or_default();
            }

            // 2) add missing keys from example
            if example_exists
                && report
                    .issues
                    .iter()
                    .any(|i| matches!(i.fix, Some(DoctorFixKind::AddMissingEnvKeys)))
            {
                if let Ok(ex_c) = fs::read_to_string(example_path) {
                    let ex_vars = parse_env_vars(&ex_c);
                    let mut cur_keys: std::collections::HashSet<String> =
                        working_vars.iter().map(|v| v.key.clone()).collect();
                    let mut added = 0;
                    let mut new_vars = working_vars.clone();
                    for ev in ex_vars {
                        if !cur_keys.contains(&ev.key) {
                            cur_keys.insert(ev.key.clone());
                            new_vars.push(ev);
                            added += 1;
                        }
                    }
                    if added > 0 {
                        working_vars = new_vars;
                        if let Some(env) = self.env_states.get_mut(&product) {
                            env.vars = working_vars.clone();
                            env.dirty = true;
                        }
                        report
                            .applied
                            .push(format!("Adicionadas {added} chave(s) do .env.example"));
                        changed = true;
                    }
                }
            }

            // 3) fill secret/required auto-generatable empties
            let mut filled = 0usize;
            {
                let mut vars = working_vars.clone();
                for v in vars.iter_mut() {
                    if v.value.trim().is_empty() {
                        if v.key == "DB_PASSWORD" {
                            if let Ok(s) = secrets::generate(secrets::SecretKind::Hex(24)) {
                                v.value = s;
                                filled += 1;
                                continue;
                            }
                        }
                        if secrets::can_auto_generate(&v.key) && v.key != "TUNNEL_TOKEN" {
                            if let Some(kind) = secrets::default_kind_for(&v.key) {
                                if let Ok(s) = secrets::generate(kind) {
                                    v.value = s;
                                    filled += 1;
                                }
                            }
                        }
                    }
                }
                if filled > 0 {
                    working_vars = vars;
                    if let Some(env) = self.env_states.get_mut(&product) {
                        env.vars = working_vars.clone();
                        env.dirty = true;
                    }
                    report.applied.push(format!(
                        "Preenchidos {filled} segredo(s)/campo(s) automaticamente"
                    ));
                    changed = true;
                }
            }

            // persist env if we changed in-memory or need normalized save
            if changed
                || report
                    .issues
                    .iter()
                    .any(|i| matches!(i.fix, Some(DoctorFixKind::SaveEnvNormalized)))
            {
                let content = render_env_file(&working_vars);
                let target = Path::new(&env_path_s);
                if let Some(parent) = target.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match fs::write(target, content) {
                    Ok(_) => {
                        if let Some(env) = self.env_states.get_mut(&product) {
                            env.dirty = false;
                            env.last_action = "Doctor aplicou correções e salvou o .env".into();
                        } else {
                            self.env_states.insert(product, load_env_state(product));
                        }
                        report.applied.push(format!("Salvo products/{label}/.env"));
                        #[cfg(unix)]
                        match restrict_env_permissions(target) {
                            Ok(_) => report
                                .applied
                                .push("Permissões do .env ajustadas para 600".into()),
                            Err(e) => report
                                .applied
                                .push(format!("Falha ao ajustar permissões do .env: {e}")),
                        }
                    }
                    Err(e) => report.applied.push(format!("Falha ao salvar .env: {e}")),
                }
            } else if env_path.exists()
                && report
                    .issues
                    .iter()
                    .any(|i| matches!(i.fix, Some(DoctorFixKind::FixEnvPermissions)))
            {
                #[cfg(unix)]
                match restrict_env_permissions(env_path) {
                    Ok(_) => report
                        .applied
                        .push("Permissões do .env ajustadas para 600".into()),
                    Err(e) => report
                        .applied
                        .push(format!("Falha ao ajustar permissões do .env: {e}")),
                }
            }

            // reload env state/findings after fixes and rescan for up-to-date report
            self.env_states.insert(product, load_env_state(product));
            self.refresh_onboarding();

            let applied = report.applied.clone();
            self.run_doctor_scan(false);
            self.doctor.applied = applied;
            let errs = self
                .doctor
                .issues
                .iter()
                .filter(|i| matches!(i.severity, PreflightStatus::Error))
                .count();
            let warns = self
                .doctor
                .issues
                .iter()
                .filter(|i| matches!(i.severity, PreflightStatus::Warn))
                .count();
            self.doctor.summary = if self.doctor.applied.is_empty() {
                format!(
                    "Doctor: nenhuma correção automática aplicada ({errs} erro(s), {warns} aviso(s))."
                )
            } else {
                format!(
                    "Doctor: {} correção(ões) aplicada(s). Estado atual: {errs} erro(s), {warns} aviso(s).",
                    self.doctor.applied.len()
                )
            };
            if let Some(ops) = self.ops_states.get_mut(&product) {
                ops.last_action = self.doctor.summary.clone();
                for a in &self.doctor.applied {
                    ops.log_lines.push(format!("[doctor] {a}"));
                }
            }
            if self.doctor_selected >= self.doctor.issues.len() && !self.doctor.issues.is_empty() {
                self.doctor_selected = self.doctor.issues.len() - 1;
            }
            return;
        }

        let errs = report
            .issues
            .iter()
            .filter(|i| matches!(i.severity, PreflightStatus::Error))
            .count();
        let warns = report
            .issues
            .iter()
            .filter(|i| matches!(i.severity, PreflightStatus::Warn))
            .count();
        let fixable = report.issues.iter().filter(|i| i.fix.is_some()).count();
        report.summary = if apply_fixes {
            if report.applied.is_empty() {
                format!("Doctor: nenhuma correção automática aplicada ({errs} erro(s), {warns} aviso(s)).")
            } else {
                format!("Doctor: {} correção(ões) aplicada(s). Restam {errs} erro(s), {warns} aviso(s).", report.applied.len())
            }
        } else if report.issues.is_empty() {
            "Doctor: nenhum problema encontrado.".into()
        } else {
            format!("Doctor: {errs} erro(s), {warns} aviso(s), {fixable} com correção automática. Use [K] para corrigir.")
        };

        if let Some(ops) = self.ops_states.get_mut(&product) {
            ops.last_action = report.summary.clone();
            for a in &report.applied {
                ops.log_lines.push(format!("[doctor] {a}"));
            }
        }
        self.doctor = report;
        if self.doctor_selected >= self.doctor.issues.len() && !self.doctor.issues.is_empty() {
            self.doctor_selected = self.doctor.issues.len() - 1;
        }
    }

    pub fn run_preflight(&mut self) {
        let product = self.featured_product;
        let label = product.label();
        let mut checks = Vec::new();
        let env_path = Path::new("products").join(label).join(".env");
        let example_path = Path::new("products").join(label).join(".env.example");
        let compose_base = Path::new("common/compose.base.yaml");
        let dc = Path::new("common/dc.sh");

        checks.push(if compose_base.exists() {
            PreflightCheck {
                name: "Compose base".into(),
                status: PreflightStatus::Ok,
                detail: "common/compose.base.yaml encontrado".into(),
            }
        } else {
            PreflightCheck {
                name: "Compose base".into(),
                status: PreflightStatus::Error,
                detail: "common/compose.base.yaml ausente".into(),
            }
        });
        checks.push(if dc.exists() {
            PreflightCheck {
                name: "Script de orquestração".into(),
                status: PreflightStatus::Ok,
                detail: "common/dc.sh disponível".into(),
            }
        } else {
            PreflightCheck {
                name: "Script de orquestração".into(),
                status: PreflightStatus::Error,
                detail: "common/dc.sh ausente".into(),
            }
        });
        checks.push(if example_path.exists() {
            PreflightCheck {
                name: "Modelo .env".into(),
                status: PreflightStatus::Ok,
                detail: format!("products/{label}/.env.example encontrado"),
            }
        } else {
            PreflightCheck {
                name: "Modelo .env".into(),
                status: PreflightStatus::Warn,
                detail: "arquivo .env.example não encontrado".into(),
            }
        });
        checks.push(if env_path.exists() {
            PreflightCheck {
                name: "Arquivo .env".into(),
                status: PreflightStatus::Ok,
                detail: format!("products/{label}/.env presente"),
            }
        } else {
            PreflightCheck {
                name: "Arquivo .env".into(),
                status: PreflightStatus::Error,
                detail: "gere o .env a partir do exemplo antes de subir".into(),
            }
        });

        if let Ok(content) = fs::read_to_string(&env_path) {
            let ef = envfile::EnvFile::parse(&content);
            let other = fs::read_to_string(product_env_path(product.toggle()))
                .ok()
                .map(|c| envfile::EnvFile::parse(&c));
            let findings = validate::validate_env(&ef, &env_path, other.as_ref());
            let errors = findings
                .iter()
                .filter(|f| matches!(f.severity, validate::Severity::Error))
                .count();
            let warns = findings
                .iter()
                .filter(|f| matches!(f.severity, validate::Severity::Warn))
                .count();
            let (st, detail) = if errors > 0 {
                (
                    PreflightStatus::Error,
                    format!("{errors} erro(s) e {warns} aviso(s) no .env"),
                )
            } else if warns > 0 {
                (
                    PreflightStatus::Warn,
                    format!("{warns} aviso(s) no .env, sem erros bloqueantes"),
                )
            } else {
                (PreflightStatus::Ok, "validações do .env sem alertas".into())
            };
            checks.push(PreflightCheck {
                name: "Validação do .env".into(),
                status: st,
                detail,
            });
            if let Ok(ex) = fs::read_to_string(&example_path) {
                let drift = validate::drift(&ef, &envfile::EnvFile::parse(&ex));
                if drift.is_empty() {
                    checks.push(PreflightCheck {
                        name: "Drift .env".into(),
                        status: PreflightStatus::Ok,
                        detail: "alinhado com .env.example".into(),
                    });
                } else {
                    checks.push(PreflightCheck {
                        name: "Drift .env".into(),
                        status: PreflightStatus::Warn,
                        detail: format!("{} diferença(s) em relação ao exemplo", drift.len()),
                    });
                }
            }
        }

        let bfind = validate::validate_build(Path::new("products").join(label).as_path());
        checks.push(if bfind.is_empty() {
            PreflightCheck {
                name: "Build do produto".into(),
                status: PreflightStatus::Ok,
                detail: "build.env/apps.json sem alertas".into(),
            }
        } else {
            PreflightCheck {
                name: "Build do produto".into(),
                status: PreflightStatus::Warn,
                detail: format!("{} alerta(s) em build.env/apps.json", bfind.len()),
            }
        });

        self.preflight = checks;
        self.preflight_ran = true;
        self.refresh_onboarding();
        if let Some(ops) = self.ops_states.get_mut(&product) {
            let errs = self
                .preflight
                .iter()
                .filter(|c| matches!(c.status, PreflightStatus::Error))
                .count();
            ops.last_action = if errs == 0 {
                "pré-checagem concluída: pronto ou com avisos".into()
            } else {
                format!("pré-checagem: {errs} item(ns) exigem atenção")
            };
        }
    }

    pub fn refresh_onboarding(&mut self) {
        let product = self.featured_product;
        let label = product.label();
        let env_path = Path::new("products").join(label).join(".env");
        let example_path = Path::new("products").join(label).join(".env.example");
        let env = self.env_states.get(&product);
        let has_env = env_path.exists();
        let has_example = example_path.exists();
        let tunnel_ok = env_has_tunnel_token(product);
        let critical_missing = env
            .map(|e| {
                e.vars.iter().any(|v| {
                    ["IMAGE_REF", "DB_PASSWORD", "TUNNEL_TOKEN"].contains(&v.key.as_str())
                        && v.value.trim().is_empty()
                })
            })
            .unwrap_or(true);
        let pre_ok = self.preflight_ran
            && !self
                .preflight
                .iter()
                .any(|c| matches!(c.status, PreflightStatus::Error));

        self.onboarding_steps = vec![
            OnboardingStep {
                title: "1. Conferir produto e arquivos".into(),
                done: has_example,
                detail: if has_example {
                    format!("Modelo products/{label}/.env.example disponível")
                } else {
                    "Modelo .env.example não encontrado".into()
                },
            },
            OnboardingStep {
                title: "2. Gerar e ajustar .env".into(),
                done: has_env && !critical_missing,
                detail: if !has_env {
                    "Em [3] Bridges & Plugins > Env use [g] para gerar do exemplo".into()
                } else if critical_missing {
                    "Preencha IMAGE_REF, DB_PASSWORD e TUNNEL_TOKEN".into()
                } else {
                    "Variáveis essenciais presentes".into()
                },
            },
            OnboardingStep {
                title: "3. Configurar túnel Cloudflare".into(),
                done: tunnel_ok,
                detail: if tunnel_ok {
                    "TUNNEL_TOKEN configurado".into()
                } else {
                    "Informe TUNNEL_TOKEN em [3] Bridges & Plugins".into()
                },
            },
            OnboardingStep {
                title: "4. Rodar pré-checagem".into(),
                done: pre_ok,
                detail: if pre_ok {
                    "Pré-checagem sem erros bloqueantes".into()
                } else {
                    "Em [2] Compose Stacks > Operações execute [f]".into()
                },
            },
            OnboardingStep {
                title: "5. Subir e consultar a stack".into(),
                done: matches!(
                    self.ops_states.get(&product).map(|o| o.state),
                    Some(StackState::Running)
                ),
                detail: "Use [u] Subir e [c] Consultar em [2] > Operações (execução real).".into(),
            },
        ];
    }

    // --- Add node modal ---
    fn handle_add_node_key(&mut self, key: KeyEvent) -> Option<Action> {
        let Modal::AddNode(ref mut form) = self.modal else {
            return None;
        };
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
            }
            KeyCode::Tab => {
                form.field = match form.field {
                    AddField::Name => AddField::Host,
                    AddField::Host => AddField::User,
                    AddField::User => AddField::Provider,
                    AddField::Provider => AddField::Cost,
                    AddField::Cost => AddField::Name,
                };
            }
            KeyCode::Backspace => {
                Self::active_field_mut(form).pop();
            }
            KeyCode::Enter => {
                if let Ok(cost) = form.cost.trim().parse::<f64>() {
                    let idx = self.nodes.len() + 1;
                    let name = if form.name.trim().is_empty() {
                        format!("node-{idx}")
                    } else {
                        form.name.trim().to_string()
                    };
                    self.nodes.push(Node {
                        id: format!("n{idx}"),
                        name,
                        region: "custom".into(),
                        host: form.host.trim().to_string(),
                        ssh_user: form.user.trim().to_string(),
                        provider: form.provider.trim().to_string(),
                        monthly_cost: cost.max(0.0),
                        online: true,
                        cpu_pct: 10.0,
                        ram_pct: 20.0,
                        cpu_history: vec![10],
                        ram_history: vec![20],
                    });
                    self.selected_node = self.nodes.len().saturating_sub(1);
                    self.selected_container = 0;
                }
                self.modal = Modal::None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                Self::active_field_mut(form).push(c);
            }
            _ => {}
        }
        None
    }
    fn active_field_mut(form: &mut AddNodeForm) -> &mut String {
        match form.field {
            AddField::Name => &mut form.name,
            AddField::Host => &mut form.host,
            AddField::User => &mut form.user,
            AddField::Provider => &mut form.provider,
            AddField::Cost => &mut form.cost,
        }
    }

    fn handle_drop_node_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.modal = Modal::None;
            }
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                if let Modal::DropNode { ref mut confirm } = self.modal {
                    *confirm = !*confirm;
                }
            }
            KeyCode::Enter => {
                if let Modal::DropNode { confirm: true } = self.modal {
                    if !self.nodes.is_empty() {
                        let node_id = self.nodes[self.selected_node].id.clone();
                        self.nodes.remove(self.selected_node);
                        self.containers.retain(|c| c.node_id != node_id);
                        if self.selected_node >= self.nodes.len() && !self.nodes.is_empty() {
                            self.selected_node = self.nodes.len() - 1;
                        } else if self.nodes.is_empty() {
                            self.selected_node = 0;
                        }
                        self.selected_container = 0;
                    }
                }
                self.modal = Modal::None;
            }
            KeyCode::Char('y') => {
                if let Modal::DropNode { ref mut confirm } = self.modal {
                    *confirm = true;
                }
            }
            _ => {}
        }
        None
    }

    // --- Add bench app modal ---
    fn handle_add_app_key(&mut self, key: KeyEvent) -> Option<Action> {
        let Modal::AddApp(ref mut form) = self.modal else {
            return None;
        };
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
            }
            KeyCode::Tab => {
                form.field = match form.field {
                    AppField::Url => AppField::Branch,
                    AppField::Branch => AppField::Url,
                };
            }
            KeyCode::Backspace => {
                match form.field {
                    AppField::Url => form.url.pop(),
                    AppField::Branch => form.branch.pop(),
                };
            }
            KeyCode::Enter => {
                let url = form.url.trim().to_string();
                let branch = form.branch.trim().to_string();
                if url.is_empty() || branch.is_empty() {
                    self.set_apps_status(
                        "informe a URL do repositório e a referência (tag/commit)".into(),
                    );
                    return None;
                }
                match self.add_app_entry(&url, &branch) {
                    Ok(()) => {
                        self.modal = Modal::None;
                    }
                    Err(e) => {
                        self.set_apps_status(format!("não foi possível salvar apps.json: {e}"));
                    }
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                match form.field {
                    AppField::Url => form.url.push(c),
                    AppField::Branch => form.branch.push(c),
                }
            }
            _ => {}
        }
        None
    }

    pub fn apps_for_featured(&self) -> Vec<(String, String)> {
        match self.load_apps_json() {
            Ok(arr) => apps_from_json_array(&arr),
            Err(_) => vec![],
        }
    }

    pub fn apps_load_error(&self) -> Option<String> {
        self.load_apps_json().err()
    }

    fn apps_json_path(&self) -> std::path::PathBuf {
        Path::new("products")
            .join(self.featured_product.label())
            .join("apps.json")
    }

    fn load_apps_json(&self) -> Result<Vec<serde_json::Value>, String> {
        let path = self.apps_json_path();
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(format!("falha ao ler {}: {e}", path.display())),
        };
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("apps.json inválido em {}: {e}", path.display()))?;
        match v {
            serde_json::Value::Array(arr) => Ok(arr),
            _ => Err(format!(
                "apps.json inválido em {}: esperado uma lista",
                path.display()
            )),
        }
    }

    fn write_apps_json(&self, path: &Path, arr: &[serde_json::Value]) -> Result<(), String> {
        let out = serde_json::to_string_pretty(arr)
            .map_err(|e| format!("falha ao preparar apps.json: {e}"))?;
        fs::write(path, out + "\n").map_err(|e| format!("falha ao salvar {}: {e}", path.display()))
    }

    fn set_apps_status(&mut self, msg: String) {
        if let Some(ops) = self.ops_states.get_mut(&self.featured_product) {
            ops.last_action = msg.clone();
            ops.log_lines.push(format!("[apps] {msg}"));
        }
    }

    fn add_app_entry(&mut self, url: &str, branch: &str) -> Result<(), String> {
        let path = self.apps_json_path();
        let mut arr = self.load_apps_json()?;
        arr.push(serde_json::json!({"url": url, "branch": branch}));
        self.write_apps_json(&path, &arr)?;
        self.apps_selected = arr.len().saturating_sub(1);
        self.set_apps_status("aplicativo adicionado ao bench (apps.json)".into());
        if let Some(ops) = self.ops_states.get_mut(&self.featured_product) {
            ops.log_lines
                .push(format!("[apps] repo={url} ref={branch}"));
        }
        Ok(())
    }

    fn remove_selected_app(&mut self) {
        let path = self.apps_json_path();
        let mut arr = match self.load_apps_json() {
            Ok(a) => a,
            Err(e) => {
                self.set_apps_status(format!("não foi possível remover: {e}"));
                return;
            }
        };
        if arr.is_empty() || self.apps_selected >= arr.len() {
            return;
        }
        arr.remove(self.apps_selected);
        if self.apps_selected >= arr.len() && !arr.is_empty() {
            self.apps_selected = arr.len() - 1;
        }
        if let Err(e) = self.write_apps_json(&path, &arr) {
            self.set_apps_status(format!("não foi possível salvar remoção em apps.json: {e}"));
            return;
        }
        self.set_apps_status("aplicativo removido do bench (apps.json)".into());
    }

    pub fn build_hints(&self) -> String {
        let p = self.featured_product.label();
        let path = Path::new("products").join(p).join("build.env");
        let content =
            fs::read_to_string(path).unwrap_or_else(|_| "(build.env não encontrado)".into());
        format!("Produto: {p}\nArquivo: products/{p}/build.env\n\n{content}\nDica: FRAPPE_REF deve ser tag ou commit fixo (não branch móvel). Para adicionar apps no bench, informe o repositório Git (URL) e a tag/commit em apps.json.")
    }

    pub fn compose_yaml_for_selected(&self) -> String {
        let Some(node) = self.selected_node() else {
            return "# no node selected".into();
        };
        let containers: Vec<&Container> = self
            .containers
            .iter()
            .filter(|c| c.node_id == node.id)
            .collect();
        let mut y = String::from("version: \"3.9\"\nservices:\n");
        for c in &containers {
            let est = (c.cpu_share_pct + c.ram_share_pct) / 200.0 * node.monthly_cost;
            y.push_str(&format!(
                "  {}:\n    image: example/{}:latest\n    x-provider:\n      node: {}\n      region: {}\n    x-finops:\n      monthly_estimate: \"${:.2}\"\n",
                c.name, c.name, node.name, node.region, est
            ));
        }
        if containers.is_empty() {
            y.push_str("  {}\n");
        }
        y
    }

    pub fn featured_compose_preview(&self) -> String {
        let p = self.featured_product.label();
        format!(
            "# Stack: {p} (common/dc.sh {p} ...)\n# Base: common/compose.base.yaml  Env: products/{p}/.env\n\nservices (base): configurator, backend, websocket, queue-short, queue-long, scheduler, frontend, db, redis-cache, redis-queue, cloudflared\n\nx-product:\n  name: {p}\n  ops:\n    up: common/dc.sh {p} up -d\n    down: common/dc.sh {p} down\n    ps/logs: common/dc.sh {p} ps|logs\n    migrate: common/dc.sh {p} run --rm ops bench --site <SITE> migrate\n"
        )
    }

    pub fn env_hint_for_key(key: &str) -> &'static str {
        let k = key.to_ascii_uppercase();
        if k == "IMAGE_REF" {
            return "Imagem do produto (caminho/registry). Em produção use tag fixa ou digest @sha256:...";
        }
        if k == "DB_PASSWORD" {
            return "Senha do banco. Gere um segredo forte (hex 24 bytes recomendado).";
        }
        if k == "TUNNEL_TOKEN" {
            return "Token do Cloudflare Tunnel (copie do painel Zero Trust).";
        }
        if k == "CLOUDFLARED_VERSION" {
            return "Obsoleto: a versão fixa foi descontinuada. O túnel usa cloudflare/cloudflared:latest com --token.";
        }
        if k.contains("SUBNET") || k.contains("IP_RANGE") || k.contains("CLOUDFLARED_IP") {
            return "Endereço/rede IPv4 (CIDR ou IP). Mantenha consistência e sem conflito entre produtos.";
        }
        if k.contains("URL") || k.contains("HOST") || k.contains("ENDPOINT") {
            return "URL ou host (ex: https://exemplo.com ou nome/IP).";
        }
        if k.contains("PATH") || k.contains("DIR") || k.contains("FILE") {
            return "Caminho de arquivo ou diretório.";
        }
        if k.contains("TOKEN")
            || k.contains("SECRET")
            || k.contains("PASSWORD")
            || k.contains("KEY")
        {
            return "Segredo/credencial. Gere automaticamente quando possível e evite expor.";
        }
        if k.contains("VERSION") || k.contains("REF") || k.contains("TAG") || k.contains("BRANCH") {
            return "Versão, tag ou referência fixa (commit/tag) para reprodutibilidade.";
        }
        "Valor de configuração. Informe o dado solicitado (texto, URL, caminho, etc.)."
    }
}

pub fn product_env_path(product: Product) -> String {
    format!("products/{}/.env", product.label())
}
pub fn product_env_example_path(product: Product) -> String {
    format!("products/{}/.env.example", product.label())
}

pub fn parse_env_vars(content: &str) -> Vec<EnvVar> {
    let mut vars = Vec::new();
    let mut pending_comment: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            pending_comment = None;
            continue;
        }
        if trimmed.starts_with('#') {
            pending_comment = Some(trimmed.trim_start_matches('#').trim().to_string());
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            vars.push(EnvVar {
                key: k.trim().to_string(),
                value: v.to_string(),
                comment: pending_comment.take(),
            });
        } else {
            vars.push(EnvVar {
                key: line.to_string(),
                value: String::new(),
                comment: pending_comment.take(),
            });
        }
    }
    vars
}

pub fn render_env_file(vars: &[EnvVar]) -> String {
    let mut out = String::new();
    for v in vars {
        if let Some(c) = &v.comment {
            if !c.is_empty() {
                out.push_str(&format!("# {c}\n"));
            }
        }
        out.push_str(&format!("{}={}\n", v.key, v.value));
    }
    out
}

pub fn load_env_state(product: Product) -> ProductEnv {
    let env_path = product_env_path(product);
    let example_path = product_env_example_path(product);
    let (path_hint, content, last_action, loaded_from_env) = if Path::new(&env_path).exists() {
        match fs::read_to_string(&env_path) {
            Ok(c) => (
                env_path.clone(),
                c,
                format!("carregado de {env_path}"),
                true,
            ),
            Err(e) => (
                env_path.clone(),
                String::new(),
                format!("erro ao ler {env_path}: {e}"),
                false,
            ),
        }
    } else if Path::new(&example_path).exists() {
        match fs::read_to_string(&example_path) {
            Ok(c) => (
                env_path.clone(),
                c,
                format!("baseado em {example_path} (gere/salve .env)"),
                false,
            ),
            Err(e) => (
                env_path.clone(),
                String::new(),
                format!("erro ao ler {example_path}: {e}"),
                false,
            ),
        }
    } else {
        (
            env_path.clone(),
            String::new(),
            "nenhum .env/.env.example encontrado".into(),
            false,
        )
    };
    let vars = if content.is_empty() {
        Vec::new()
    } else {
        parse_env_vars(&content)
    };
    let mut findings = Vec::new();
    if !content.is_empty() {
        let ef = envfile::EnvFile::parse(&content);
        let other_env = fs::read_to_string(product_env_path(product.toggle()))
            .ok()
            .map(|c| envfile::EnvFile::parse(&c));
        let env_for_validation = if loaded_from_env || Path::new(&env_path).exists() {
            fs::read_to_string(&env_path)
                .ok()
                .map(|c| envfile::EnvFile::parse(&c))
                .unwrap_or_else(|| envfile::EnvFile::parse(&content))
        } else {
            ef.clone()
        };
        let path = Path::new(&env_path);
        for fnd in validate::validate_env(&env_for_validation, path, other_env.as_ref()) {
            findings.push(format!(
                "{:?} {}: {}",
                fnd.severity,
                fnd.key.unwrap_or_else(|| "-".into()),
                fnd.message
            ));
        }
        if let Ok(ex) = fs::read_to_string(&example_path) {
            for fnd in validate::drift(&env_for_validation, &envfile::EnvFile::parse(&ex)) {
                findings.push(format!(
                    "{:?} {}: {}",
                    fnd.severity,
                    fnd.key.unwrap_or_else(|| "-".into()),
                    fnd.message
                ));
            }
        }
        let prod_dir = Path::new("products").join(product.label());
        for fnd in validate::validate_build(&prod_dir) {
            findings.push(format!(
                "{:?} {}: {}",
                fnd.severity,
                fnd.key.unwrap_or_else(|| "-".into()),
                fnd.message
            ));
        }
    }
    ProductEnv {
        product,
        path_hint,
        vars,
        dirty: false,
        last_action,
        reveal_secrets: false,
        findings,
    }
}

pub fn env_has_tunnel_token(product: Product) -> bool {
    let env = load_env_state(product);
    env.vars
        .iter()
        .find(|v| v.key == "TUNNEL_TOKEN")
        .map(|v| !v.value.trim().is_empty())
        .unwrap_or(false)
}

fn apps_from_json_array(arr: &[serde_json::Value]) -> Vec<(String, String)> {
    arr.iter()
        .map(|a| {
            (
                a.get("url")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                a.get("branch")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect()
}

// (token editing is available through the .env lifecycle view)
