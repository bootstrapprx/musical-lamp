use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, Event};
use crate::domain::{
    allocate_costs, Container, EnvVar, Node, OnboardingStep, OpsStatus, PreflightCheck,
    PreflightStatus, Product, ProductEnv, StackState,
};
use crate::ops_core::{envfile, secrets, validate};
use crate::telemetry::TelemetryProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    ClusterFinOps,
    ComposeStacks,
    BridgesPlugins,
    EnvLifecycle,
    Operations,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    None,
    AddNode(AddNodeForm),
    DropNode { confirm: bool },
    TunnelToken { input: String, product: Product },
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

pub struct AppState {
    pub should_quit: bool,
    pub view: View,
    pub focus: FocusPane,
    pub detail_tab: DetailTab,
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
        ops_states.insert(
            Product::Accounting,
            OpsStatus {
                product: Product::Accounting,
                state: StackState::Running,
                last_action: "status ok (mock)".into(),
                log_lines: vec!["[accounting] stack saudável".into()],
            },
        );
        ops_states.insert(
            Product::Trivia,
            OpsStatus {
                product: Product::Trivia,
                state: StackState::Running,
                last_action: "status ok (mock)".into(),
                log_lines: vec!["[trivia] stack saudável".into()],
            },
        );

        let mut s = Self {
            should_quit: false,
            view: View::ClusterFinOps,
            focus: FocusPane::Nodes,
            detail_tab: DetailTab::Logs,
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
        };
        s.refresh_onboarding();
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
                self.tick_ops_sim();
                None
            }
            Event::Resize => None,
            Event::Key(key) => self.handle_key(key),
        }
    }

    fn tick_ops_sim(&mut self) {
        for ops in self.ops_states.values_mut() {
            match ops.state {
                StackState::Starting => {
                    ops.state = StackState::Running;
                    ops.last_action = "inicialização concluída (simulada)".into();
                    ops.log_lines.push("[ops] serviços prontos".into());
                }
                StackState::Stopping => {
                    ops.state = StackState::Stopped;
                    ops.last_action = "parada concluída (simulada)".into();
                    ops.log_lines.push("[ops] stack parado".into());
                }
                _ => {}
            }
            if ops.log_lines.len() > 80 {
                let drain = ops.log_lines.len() - 80;
                ops.log_lines.drain(0..drain);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if matches!(self.modal, Modal::AddNode(_)) {
            return self.handle_add_node_key(key);
        }
        if matches!(self.modal, Modal::DropNode { .. }) {
            return self.handle_drop_node_key(key);
        }
        if matches!(self.modal, Modal::TunnelToken { .. }) {
            return self.handle_tunnel_token_key(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }

        // global product switch and view keys (when not editing env value)
        if self.view == View::EnvLifecycle && self.env_editing {
            return self.handle_env_edit_key(key);
        }

        match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('p') => {
                self.featured_product = self.featured_product.toggle();
                self.clamp_env_selection();
                self.preflight.clear();
                self.preflight_ran = false;
                self.refresh_onboarding();
                if self.view == View::BridgesPlugins && !env_has_tunnel_token(self.featured_product)
                {
                    self.modal = Modal::TunnelToken {
                        input: String::new(),
                        product: self.featured_product,
                    };
                }
                None
            }
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
                    if !env_has_tunnel_token(self.featured_product) {
                        self.modal = Modal::TunnelToken {
                            input: String::new(),
                            product: self.featured_product,
                        };
                    }
                }
                None
            }
            KeyCode::Char('4') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::Forecast7d;
                } else {
                    self.view = View::EnvLifecycle;
                }
                None
            }
            KeyCode::Char('5') => {
                self.view = View::Operations;
                None
            }
            KeyCode::Tab => {
                if self.view == View::ClusterFinOps {
                    self.focus = match self.focus {
                        FocusPane::Nodes => FocusPane::Containers,
                        FocusPane::Containers => FocusPane::Detail,
                        FocusPane::Detail => FocusPane::Nodes,
                    };
                }
                None
            }
            KeyCode::Char('l')
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail =>
            {
                self.detail_tab = DetailTab::Logs;
                None
            }
            KeyCode::Char('b')
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail =>
            {
                self.detail_tab = DetailTab::CostBreakdown;
                None
            }
            KeyCode::Char('y')
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail =>
            {
                self.detail_tab = DetailTab::ComposeYaml;
                None
            }
            KeyCode::Char('f')
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail =>
            {
                self.detail_tab = DetailTab::Forecast7d;
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
            KeyCode::Enter if self.view == View::EnvLifecycle => {
                self.start_env_edit();
                None
            }
            KeyCode::Char('g') if self.view == View::EnvLifecycle => {
                self.generate_env_from_example();
                None
            }
            KeyCode::Char('r') if self.view == View::EnvLifecycle => {
                self.reload_env();
                None
            }
            KeyCode::Char('w') if self.view == View::EnvLifecycle => {
                self.save_env();
                None
            }
            KeyCode::Char('v') if self.view == View::EnvLifecycle => {
                self.toggle_env_reveal();
                None
            }
            KeyCode::Char('n') if self.view == View::EnvLifecycle => {
                self.generate_secret_for_selected();
                None
            }
            KeyCode::Char('u') if self.view == View::Operations => {
                self.run_ops_action("up");
                None
            }
            KeyCode::Char('x') if self.view == View::Operations => {
                self.run_ops_action("down");
                None
            }
            KeyCode::Char('c') if self.view == View::Operations => {
                self.run_ops_action("status");
                None
            }
            KeyCode::Char('f') if self.view == View::Operations => {
                self.run_ops_action("preflight");
                None
            }
            KeyCode::Char('i') if self.view == View::Operations => {
                self.run_ops_action("onboarding");
                None
            }
            KeyCode::Char('m') if self.view == View::Operations => {
                self.run_ops_action("migrate");
                None
            }
            KeyCode::Char('o') if self.view == View::Operations => {
                self.run_ops_action("logs");
                None
            }
            KeyCode::Left if self.view == View::Operations => {
                self.ops_selected_action = self.ops_selected_action.saturating_sub(1);
                None
            }
            KeyCode::Right if self.view == View::Operations => {
                if self.ops_selected_action < 6 {
                    self.ops_selected_action += 1;
                }
                None
            }
            KeyCode::Enter if self.view == View::Operations => {
                self.run_selected_ops_action();
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
                    if self.selected_container > 0 {
                        self.selected_container -= 1;
                    }
                }
                FocusPane::Detail => {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                }
            },
            View::EnvLifecycle => {
                if self.env_selected > 0 {
                    self.env_selected -= 1;
                }
            }
            View::Operations if self.ops_selected_action > 0 => {
                self.ops_selected_action -= 1;
            }
            _ => {}
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
            View::EnvLifecycle => {
                if let Some(env) = self.current_env() {
                    if self.env_selected + 1 < env.vars.len() {
                        self.env_selected += 1;
                    }
                }
            }
            View::Operations if self.ops_selected_action < 6 => {
                self.ops_selected_action += 1;
            }
            _ => {}
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
        let (idx, val) = if let Some(env) = self.current_env() {
            (
                self.env_selected,
                env.vars
                    .get(self.env_selected)
                    .map(|v| v.value.clone())
                    .unwrap_or_default(),
            )
        } else {
            return;
        };
        self.env_selected = idx;
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
        let msg = match fs::read_to_string(&example) {
            Ok(content) => {
                let vars = parse_env_vars(&content);
                if let Some(env) = self.env_states.get_mut(&product) {
                    env.vars = vars;
                    env.dirty = true;
                    env.last_action = format!("gerado a partir de {} (não salvo)", example);
                    env.path_hint = target;
                }
                "gerado".into()
            }
            Err(e) => format!("falha ao ler exemplo: {e}"),
        };
        if let Some(env) = self.env_states.get_mut(&product) {
            if msg.starts_with("falha") {
                env.last_action = msg;
            }
        }
        self.refresh_onboarding();
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
        let loaded = load_env_state(product);
        self.env_states.insert(product, loaded);
        self.clamp_env_selection();
        self.refresh_onboarding();
    }

    fn save_env(&mut self) {
        let product = self.featured_product;
        let path = product_env_path(product);
        let (content, can_save) = if let Some(env) = self.current_env() {
            (render_env_file(&env.vars), true)
        } else {
            (String::new(), false)
        };
        if !can_save {
            return;
        }
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
    }

    fn run_ops_action(&mut self, action: &str) {
        let product = self.featured_product;
        let label = product.label().to_string();
        match action {
            "preflight" => {
                self.run_preflight();
                return;
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
                return;
            }
            _ => {}
        }
        if let Some(ops) = self.ops_states.get_mut(&product) {
            match action {
                "up" => {
                    ops.state = StackState::Starting;
                    ops.last_action = "subir stack solicitado (simulado, execução segura)".into();
                    ops.log_lines
                        .push(format!("[subir] common/dc.sh {label} up -d"));
                }
                "down" => {
                    ops.state = StackState::Stopping;
                    ops.last_action = "parar stack solicitado (simulado, execução segura)".into();
                    ops.log_lines
                        .push(format!("[parar] common/dc.sh {label} down"));
                }
                "migrate" => {
                    ops.last_action = "migração solicitada (simulada)".into();
                    ops.log_lines.push(format!(
                        "[migrar] common/dc.sh {label} run --rm ops bench --site <SITE> migrate"
                    ));
                }
                "status" => {
                    ops.last_action = "consulta da stack atualizada (simulada)".into();
                    ops.log_lines.push(format!(
                        "[consultar] common/dc.sh {label} ps  → estado: {}",
                        ops.state.label()
                    ));
                    ops.log_lines.push(
                        "[consultar] serviços base: configurator, backend, frontend, db, redis, cloudflared"
                            .to_string(),
                    );
                }
                "logs" => {
                    ops.last_action = "logs da stack consultados (simulados)".into();
                    ops.log_lines.push(format!(
                        "[consultar] common/dc.sh {label} logs --tail=50"
                    ));
                }
                _ => {}
            }
            if ops.log_lines.len() > 120 {
                let drain = ops.log_lines.len() - 120;
                ops.log_lines.drain(0..drain);
            }
        }
    }

    fn run_selected_ops_action(&mut self) {
        let action = match self.ops_selected_action {
            0 => "up",
            1 => "down",
            2 => "status",
            3 => "preflight",
            4 => "onboarding",
            5 => "migrate",
            _ => "logs",
        };
        self.run_ops_action(action);
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
        let b_err = bfind
            .iter()
            .filter(|f| matches!(f.severity, validate::Severity::Error))
            .count();
        checks.push(if bfind.is_empty() {
            PreflightCheck {
                name: "Build do produto".into(),
                status: PreflightStatus::Ok,
                detail: "build.env/apps.json sem alertas".into(),
            }
        } else if b_err > 0 {
            PreflightCheck {
                name: "Build do produto".into(),
                status: PreflightStatus::Warn,
                detail: format!("{} alerta(s) em build.env/apps.json", bfind.len()),
            }
        } else {
            PreflightCheck {
                name: "Build do produto".into(),
                status: PreflightStatus::Warn,
                detail: format!("{} observação(ões) em build.env/apps.json", bfind.len()),
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
                "pré-checagem concluída: pronto para subir (ou com avisos)".into()
            } else {
                format!("pré-checagem concluída: {errs} item(ns) exigem atenção")
            };
            ops.log_lines
                .push(format!("[pre-flight] verificação executada para {label}"));
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
                    "Abra a aba [4] .env e use [g] para gerar a partir do exemplo".into()
                } else if critical_missing {
                    "Preencha IMAGE_REF, DB_PASSWORD e TUNNEL_TOKEN na aba [4] .env".into()
                } else {
                    "Variáveis essenciais presentes".into()
                },
            },
            OnboardingStep {
                title: "3. Configurar túnel Cloudflare".into(),
                done: tunnel_ok,
                detail: if tunnel_ok {
                    "TUNNEL_TOKEN configurado para o produto".into()
                } else {
                    "Informe o TUNNEL_TOKEN na aba [3] Bridges & Plugins ou [4] .env".into()
                },
            },
            OnboardingStep {
                title: "4. Rodar pré-checagem".into(),
                done: pre_ok,
                detail: if pre_ok {
                    "Pré-checagem sem erros bloqueantes".into()
                } else {
                    "Na aba [5] Operações execute [f] Pre-flight".into()
                },
            },
            OnboardingStep {
                title: "5. Subir e consultar a stack".into(),
                done: matches!(
                    self.ops_states.get(&product).map(|o| o.state),
                    Some(StackState::Running)
                ),
                detail: "Use [u] Subir stack e [c] Consultar stack na aba [5]".into(),
            },
        ];
    }

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
                    let region = "custom".to_string();
                    self.nodes.push(Node {
                        id: format!("n{idx}"),
                        name,
                        region,
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
                    self.modal = Modal::None;
                }
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

    fn handle_tunnel_token_key(&mut self, key: KeyEvent) -> Option<Action> {
        let (product, input_snapshot) = if let Modal::TunnelToken { product, input } = &self.modal {
            (*product, input.clone())
        } else {
            return None;
        };
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
            }
            KeyCode::Backspace => {
                if let Modal::TunnelToken { ref mut input, .. } = self.modal {
                    input.pop();
                }
            }
            KeyCode::Enter => {
                let token = input_snapshot.trim().to_string();
                if !token.is_empty() && set_tunnel_token(product, &token).is_ok() {
                    self.env_states.insert(product, load_env_state(product));
                }
                self.refresh_onboarding();
                self.modal = Modal::None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Modal::TunnelToken { ref mut input, .. } = self.modal {
                    input.push(c);
                }
            }
            _ => {}
        }
        None
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
        for c in containers {
            y.push_str(&format!("  {}:\n    image: example/{}:latest\n    x-provider:\n      node: {}\n      region: {}\n    x-finops:\n      monthly_estimate: \"${:.2}\"\n", c.name, c.name, node.name, node.region, (c.cpu_share_pct+c.ram_share_pct)/200.0*node.monthly_cost));
        }
        if y == "version: \"3.9\"\nservices:\n" {
            y.push_str("  {}\n");
        }
        y
    }

    pub fn featured_compose_preview(&self) -> String {
        let p = self.featured_product.label();
        format!(
            "# Stack: {p} (common/dc.sh {p} ...)\n# Compose base: common/compose.base.yaml\n# Env: products/{p}/.env\n\ninclude/base services:\n  - configurator\n  - backend\n  - websocket\n  - queue-short\n  - queue-long\n  - scheduler\n  - frontend\n  - db\n  - redis-cache\n  - redis-queue\n  - cloudflared\n\nx-product:\n  name: {p}\n  ops:\n    up:      common/dc.sh {p} up -d\n    down:    common/dc.sh {p} down\n    logs:    common/dc.sh {p} logs -f\n    migrate: common/dc.sh {p} run --rm ops bench --site <SITE> migrate\n"
        )
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
        let other_product = product.toggle();
        let other_env = fs::read_to_string(product_env_path(other_product))
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

pub fn set_tunnel_token(product: Product, token: &str) -> std::io::Result<()> {
    let mut env = load_env_state(product);
    if let Some(var) = env.vars.iter_mut().find(|v| v.key == "TUNNEL_TOKEN") {
        var.value = token.to_string();
    } else {
        env.vars.push(EnvVar {
            key: "TUNNEL_TOKEN".into(),
            value: token.to_string(),
            comment: Some("Cloudflare Tunnel token".into()),
        });
    }
    let content = render_env_file(&env.vars);
    fs::write(product_env_path(product), content)
}
