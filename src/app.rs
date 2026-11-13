use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, Event};
use crate::domain::{allocate_costs, Container, EnvVar, Node, OpsStatus, Product, ProductEnv, StackState};
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
            Container { id: "c1".into(), name: "web-frontend".into(), node_id: "n1".into(), stack: "trivia".into(), cpu_share_pct: 12.0, ram_share_pct: 18.0, status: "running".into(), logs: vec!["[boot] frontend ready".into()] },
            Container { id: "c2".into(), name: "api-backend".into(), node_id: "n1".into(), stack: "trivia".into(), cpu_share_pct: 14.0, ram_share_pct: 20.0, status: "running".into(), logs: vec!["[boot] api listening".into()] },
            Container { id: "c3".into(), name: "worker-jobs".into(), node_id: "n1".into(), stack: "trivia".into(), cpu_share_pct: 6.0, ram_share_pct: 8.0, status: "running".into(), logs: vec!["[boot] worker started".into()] },
            Container { id: "c4".into(), name: "accounting-web".into(), node_id: "n2".into(), stack: "accounting".into(), cpu_share_pct: 20.0, ram_share_pct: 24.0, status: "running".into(), logs: vec!["[boot] accounting ui".into()] },
            Container { id: "c5".into(), name: "postgres".into(), node_id: "n2".into(), stack: "accounting".into(), cpu_share_pct: 18.0, ram_share_pct: 26.0, status: "running".into(), logs: vec!["[boot] db ready".into()] },
            Container { id: "c6".into(), name: "redis".into(), node_id: "n2".into(), stack: "accounting".into(), cpu_share_pct: 8.0, ram_share_pct: 10.0, status: "running".into(), logs: vec!["[boot] cache warm".into()] },
        ];
        let mut env_states = HashMap::new();
        env_states.insert(Product::Accounting, load_env_state(Product::Accounting));
        env_states.insert(Product::Trivia, load_env_state(Product::Trivia));
        let mut ops_states = HashMap::new();
        ops_states.insert(Product::Accounting, OpsStatus { product: Product::Accounting, state: StackState::Running, last_action: "status ok (mock)".into(), log_lines: vec!["[accounting] stack saudável".into()] });
        ops_states.insert(Product::Trivia, OpsStatus { product: Product::Trivia, state: StackState::Running, last_action: "status ok (mock)".into(), log_lines: vec!["[trivia] stack saudável".into()] });

        Self {
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
        }
    }

    pub fn projected_total(&self) -> f64 {
        self.nodes.iter().map(|n| n.monthly_cost).sum()
    }

    pub fn selected_node(&self) -> Option<&Node> {
        self.nodes.get(self.selected_node)
    }

    pub fn containers_for_selected_node(&self) -> Vec<&Container> {
        if let Some(n) = self.selected_node() {
            self.containers.iter().filter(|c| c.node_id == n.id).collect()
        } else {
            vec![]
        }
    }

    pub fn selected_container_obj(&self) -> Option<&Container> {
        let list = self.containers_for_selected_node();
        list.get(self.selected_container).copied()
    }

    pub fn container_costs_map(&self) -> HashMap<String, f64> {
        allocate_costs(&self.nodes, &self.containers).into_iter().collect()
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
            if ops.log_lines.len() > 80 { let drain = ops.log_lines.len()-80; ops.log_lines.drain(0..drain); }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if matches!(self.modal, Modal::AddNode(_)) {
            return self.handle_add_node_key(key);
        }
        if matches!(self.modal, Modal::DropNode { .. }) {
            return self.handle_drop_node_key(key);
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
            KeyCode::Char('p') => { self.featured_product = self.featured_product.toggle(); self.clamp_env_selection(); None }
            KeyCode::Char('1') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail { self.detail_tab = DetailTab::Logs; }
                else { self.view = View::ClusterFinOps; }
                None
            }
            KeyCode::Char('2') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail { self.detail_tab = DetailTab::CostBreakdown; }
                else { self.view = View::ComposeStacks; }
                None
            }
            KeyCode::Char('3') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail { self.detail_tab = DetailTab::ComposeYaml; }
                else { self.view = View::BridgesPlugins; }
                None
            }
            KeyCode::Char('4') => {
                if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail { self.detail_tab = DetailTab::Forecast7d; }
                else { self.view = View::EnvLifecycle; }
                None
            }
            KeyCode::Char('5') => { self.view = View::Operations; None }
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
            KeyCode::Char('l') if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::Logs; None }
            KeyCode::Char('b') if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::CostBreakdown; None }
            KeyCode::Char('y') if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::ComposeYaml; None }
            KeyCode::Char('f') if self.view == View::ClusterFinOps && self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::Forecast7d; None }
            KeyCode::Up => { self.move_up(); None }
            KeyCode::Down => { self.move_down(); None }
            KeyCode::Enter if self.view == View::EnvLifecycle => { self.start_env_edit(); None }
            KeyCode::Char('g') if self.view == View::EnvLifecycle => { self.generate_env_from_example(); None }
            KeyCode::Char('r') if self.view == View::EnvLifecycle => { self.reload_env(); None }
            KeyCode::Char('w') if self.view == View::EnvLifecycle => { self.save_env(); None }
            KeyCode::Char('u') if self.view == View::Operations => { self.run_ops_action("up"); None }
            KeyCode::Char('x') if self.view == View::Operations => { self.run_ops_action("down"); None }
            KeyCode::Char('m') if self.view == View::Operations => { self.run_ops_action("migrate"); None }
            KeyCode::Char('k') if self.view == View::Operations => { self.run_ops_action("status"); None }
            KeyCode::Char('o') if self.view == View::Operations => { self.run_ops_action("logs"); None }
            KeyCode::Left if self.view == View::Operations => { self.ops_selected_action = self.ops_selected_action.saturating_sub(1); None }
            KeyCode::Right if self.view == View::Operations => { if self.ops_selected_action < 4 { self.ops_selected_action += 1; } None }
            KeyCode::Enter if self.view == View::Operations => { self.run_selected_ops_action(); None }
            KeyCode::Char('a') if self.view == View::ClusterFinOps => { self.modal = Modal::AddNode(AddNodeForm::default()); None }
            KeyCode::Char('d') if self.view == View::ClusterFinOps => {
                if !self.nodes.is_empty() { self.modal = Modal::DropNode { confirm: false }; }
                None
            }
            KeyCode::Char('s') if self.view == View::ClusterFinOps => { self.simulate_load(); None }
            _ => None,
        }
    }

    fn move_up(&mut self) {
        match self.view {
            View::ClusterFinOps => match self.focus {
                FocusPane::Nodes => { if self.selected_node > 0 { self.selected_node -= 1; self.selected_container = 0; } }
                FocusPane::Containers => { if self.selected_container > 0 { self.selected_container -= 1; } }
                FocusPane::Detail => { self.log_scroll = self.log_scroll.saturating_sub(1); }
            },
            View::EnvLifecycle => { if self.env_selected > 0 { self.env_selected -= 1; } }
            View::Operations if self.ops_selected_action > 0 => { self.ops_selected_action -= 1; }
            _ => {}
        }
    }
    fn move_down(&mut self) {
        match self.view {
            View::ClusterFinOps => match self.focus {
                FocusPane::Nodes => { if self.selected_node + 1 < self.nodes.len() { self.selected_node += 1; self.selected_container = 0; } }
                FocusPane::Containers => {
                    let len = self.containers_for_selected_node().len().saturating_add(1);
                    if self.selected_container + 1 < len { self.selected_container += 1; }
                }
                FocusPane::Detail => { self.log_scroll = self.log_scroll.saturating_add(1); }
            },
            View::EnvLifecycle => {
                if let Some(env) = self.current_env() {
                    if self.env_selected + 1 < env.vars.len() { self.env_selected += 1; }
                }
            }
            View::Operations if self.ops_selected_action < 4 => { self.ops_selected_action += 1; }
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
            if env.vars.is_empty() { self.env_selected = 0; }
            else if self.env_selected >= env.vars.len() { self.env_selected = env.vars.len()-1; }
        }
    }

    fn start_env_edit(&mut self) {
        let (idx, val) = if let Some(env) = self.current_env() { (self.env_selected, env.vars.get(self.env_selected).map(|v| v.value.clone()).unwrap_or_default()) } else { return; };
        self.env_selected = idx;
        self.env_editing = true;
        self.env_input = val;
    }

    fn handle_env_edit_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => { self.env_editing = false; self.env_input.clear(); }
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
            KeyCode::Backspace => { self.env_input.pop(); }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => { self.env_input.push(c); }
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
            if !msg.starts_with("falha") { /* ok */ } else { env.last_action = msg; }
        }
    }

    fn reload_env(&mut self) {
        let product = self.featured_product;
        let loaded = load_env_state(product);
        self.env_states.insert(product, loaded);
        self.clamp_env_selection();
    }

    fn save_env(&mut self) {
        let product = self.featured_product;
        let path = product_env_path(product);
        let (content, can_save) = if let Some(env) = self.current_env() {
            (render_env_file(&env.vars), true)
        } else { (String::new(), false) };
        if !can_save { return; }
        let result = fs::write(&path, content);
        if let Some(env) = self.current_env_mut() {
            match result {
                Ok(_) => { env.dirty = false; env.last_action = format!("salvo em {path}"); }
                Err(e) => { env.last_action = format!("erro ao salvar: {e}"); }
            }
        }
    }

    fn run_ops_action(&mut self, action: &str) {
        let product = self.featured_product;
        let label = product.label().to_string();
        if let Some(ops) = self.ops_states.get_mut(&product) {
            match action {
                "up" => {
                    ops.state = StackState::Starting;
                    ops.last_action = "subindo stack (simulado)".into();
                    ops.log_lines.push(format!("[ops] common/dc.sh {label} up -d (simulado)"));
                }
                "down" => {
                    ops.state = StackState::Stopping;
                    ops.last_action = "parando stack (simulado)".into();
                    ops.log_lines.push(format!("[ops] common/dc.sh {label} down (simulado)"));
                }
                "migrate" => {
                    ops.last_action = "migração solicitada (simulada)".into();
                    ops.log_lines.push(format!("[ops] common/dc.sh {label} run --rm ops bench --site <SITE> migrate (simulado)"));
                }
                "status" => {
                    ops.last_action = "status atualizado (simulado)".into();
                    ops.log_lines.push(format!("[ops] estado atual: {}", ops.state.label()));
                }
                "logs" => {
                    ops.last_action = "logs consultados (simulado)".into();
                    ops.log_lines.push(format!("[ops] common/dc.sh {label} logs --tail=50 (simulado)"));
                }
                _ => {}
            }
            if ops.log_lines.len() > 80 { let drain = ops.log_lines.len()-80; ops.log_lines.drain(0..drain); }
        }
    }

    fn run_selected_ops_action(&mut self) {
        let action = match self.ops_selected_action { 0 => "up", 1 => "down", 2 => "migrate", 3 => "status", _ => "logs" };
        self.run_ops_action(action);
    }

    fn handle_add_node_key(&mut self, key: KeyEvent) -> Option<Action> {
        let Modal::AddNode(ref mut form) = self.modal else { return None; };
        match key.code {
            KeyCode::Esc => { self.modal = Modal::None; }
            KeyCode::Tab => {
                form.field = match form.field {
                    AddField::Name => AddField::Host,
                    AddField::Host => AddField::User,
                    AddField::User => AddField::Provider,
                    AddField::Provider => AddField::Cost,
                    AddField::Cost => AddField::Name,
                };
            }
            KeyCode::Backspace => { Self::active_field_mut(form).pop(); }
            KeyCode::Enter => {
                if let Ok(cost) = form.cost.trim().parse::<f64>() {
                    let idx = self.nodes.len() + 1;
                    let name = if form.name.trim().is_empty() { format!("node-{idx}") } else { form.name.trim().to_string() };
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
            KeyCode::Esc | KeyCode::Char('n') => { self.modal = Modal::None; }
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                if let Modal::DropNode { ref mut confirm } = self.modal { *confirm = !*confirm; }
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
            KeyCode::Char('y') => { if let Modal::DropNode { ref mut confirm } = self.modal { *confirm = true; } }
            _ => {}
        }
        None
    }

    pub fn compose_yaml_for_selected(&self) -> String {
        let Some(node) = self.selected_node() else { return "# no node selected".into(); };
        let containers: Vec<&Container> = self.containers.iter().filter(|c| c.node_id == node.id).collect();
        let mut y = String::from("version: \"3.9\"\nservices:\n");
        for c in containers {
            y.push_str(&format!("  {}:\n    image: example/{}:latest\n    x-provider:\n      node: {}\n      region: {}\n    x-finops:\n      monthly_estimate: \"${:.2}\"\n", c.name, c.name, node.name, node.region, (c.cpu_share_pct+c.ram_share_pct)/200.0*node.monthly_cost));
        }
        if y == "version: \"3.9\"\nservices:\n" { y.push_str("  {}\n"); }
        y
    }

    pub fn featured_compose_preview(&self) -> String {
        let p = self.featured_product.label();
        format!(
            "# Stack: {p} (common/dc.sh {p} ...)\n# Compose base: common/compose.base.yaml\n# Env: products/{p}/.env\n\ninclude/base services:\n  - configurator\n  - backend\n  - websocket\n  - queue-short\n  - queue-long\n  - scheduler\n  - frontend\n  - db\n  - redis-cache\n  - redis-queue\n  - cloudflared\n\nx-product:\n  name: {p}\n  ops:\n    up:      common/dc.sh {p} up -d\n    down:    common/dc.sh {p} down\n    logs:    common/dc.sh {p} logs -f\n    migrate: common/dc.sh {p} run --rm ops bench --site <SITE> migrate\n"
        )
    }
}

fn product_env_path(product: Product) -> String {
    format!("products/{}/.env", product.label())
}

fn product_env_example_path(product: Product) -> String {
    format!("products/{}/.env.example", product.label())
}

fn parse_env_vars(content: &str) -> Vec<EnvVar> {
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
            vars.push(EnvVar { key: k.trim().to_string(), value: v.to_string(), comment: pending_comment.take() });
        } else {
            vars.push(EnvVar { key: line.to_string(), value: String::new(), comment: pending_comment.take() });
        }
    }
    vars
}

fn render_env_file(vars: &[EnvVar]) -> String {
    let mut out = String::new();
    for v in vars {
        if let Some(c) = &v.comment {
            if !c.is_empty() { out.push_str(&format!("# {c}\n")); }
        }
        out.push_str(&format!("{}={}\n", v.key, v.value));
    }
    out
}

fn load_env_state(product: Product) -> ProductEnv {
    let env_path = product_env_path(product);
    let example_path = product_env_example_path(product);
    let (path_hint, content, last_action) = if Path::new(&env_path).exists() {
        match fs::read_to_string(&env_path) {
            Ok(c) => (env_path.clone(), c, format!("carregado de {env_path}")),
            Err(e) => (env_path.clone(), String::new(), format!("erro ao ler {env_path}: {e}")),
        }
    } else if Path::new(&example_path).exists() {
        match fs::read_to_string(&example_path) {
            Ok(c) => (env_path.clone(), c, format!("baseado em {example_path} (gere/salve .env)")),
            Err(e) => (env_path.clone(), String::new(), format!("erro ao ler {example_path}: {e}")),
        }
    } else {
        (env_path.clone(), String::new(), "nenhum .env/.env.example encontrado".into())
    };
    let vars = if content.is_empty() { Vec::new() } else { parse_env_vars(&content) };
    ProductEnv { product, path_hint, vars, dirty: false, last_action }
}
