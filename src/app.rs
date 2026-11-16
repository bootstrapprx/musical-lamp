use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::control_plane::ControlPlane;
use crate::domain::{
    allocate_costs, discover_products, other_product, Container, DoctorFixKind, DoctorIssue,
    DoctorReport, EnvVar, Node, OnboardingStep, OpsStatus, PreflightCheck, PreflightStatus,
    Product, ProductEnv, StackState,
};
use crate::registry::AppRegistry;
use crate::state::StateStore;

use crate::event::{Action, Event};
use crate::ops_core::{envfile, secrets, validate};
use crate::orchestration;
use crate::telemetry::TelemetryProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    ClusterFinOps,
    ComposeStacks,
    BridgesPlugins,
    ProductLifecycle,
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
    BuildEnv,
    Tunnel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetEnvMode {
    RestoreExample,
    RemoveEnv,
    RestoreBackup,
    RestoreNewProductDraft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetEnvForm {
    pub product: Product,
    pub mode: ResetEnvMode,
    pub confirm_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    None,
    AddNode(AddNodeForm),
    DropNode {
        confirm: bool,
    },
    AddApp(AddAppForm),
    NewProduct(NewProductForm),
    ResetEnv(ResetEnvForm),
    ConfirmText {
        title: String,
        message: String,
        required: String,
        input: String,
        action: ConfirmAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewProductField {
    Id,
    ImageRef,
    DbPassword,
    TunnelToken,
    EdgeSubnet,
    EdgeIpRange,
    CloudflaredIp,
    FrappeRef,
    AppRepoUrl,
    AppRepoRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProductDraft {
    pub id: String,
    pub image_ref: String,
    pub db_password: String,
    pub tunnel_token: String,
    pub edge_subnet: String,
    pub edge_ip_range: String,
    pub cloudflared_ip: String,
    pub frappe_ref: String,
    pub app_repo_url: String,
    pub app_repo_ref: String,
}

impl Default for NewProductDraft {
    fn default() -> Self {
        Self {
            id: String::new(),
            image_ref: "kodoo/new-product:latest".into(),
            db_password: String::new(),
            tunnel_token: String::new(),
            edge_subnet: "172.30.10.0/28".into(),
            edge_ip_range: "172.30.10.8/29".into(),
            cloudflared_ip: "172.30.10.2".into(),
            frappe_ref: String::new(),
            app_repo_url: String::new(),
            app_repo_ref: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProductForm {
    pub field: NewProductField,
    pub draft: NewProductDraft,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    OverwriteEnvFromExample(Product),
    QuitApp,
    SwitchProduct(Product),
    RunOps { product: Product, action: String },
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
    Ref,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefMode {
    Dropdown,
    CustomInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefOption {
    pub label: String,
    pub value: String,
    pub awaited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddAppForm {
    pub url: String,
    pub ref_value: String,
    pub field: AppField,
    pub error: Option<String>,
    pub ref_mode: RefMode,
    pub ref_options: Vec<RefOption>,
    pub ref_dropdown_open: bool,
    pub ref_selected: usize,
}

impl AddAppForm {
    pub fn new(product: Product) -> Self {
        let ref_options = AppState::ref_options_for_product(&product);
        let ref_selected = 0;
        let ref_value = ref_options
            .first()
            .filter(|o| !o.value.is_empty())
            .map(|o| o.value.clone())
            .unwrap_or_default();
        let ref_mode = if ref_options.is_empty() {
            RefMode::CustomInput
        } else {
            RefMode::Dropdown
        };
        Self {
            url: String::new(),
            ref_value,
            field: AppField::Url,
            error: None,
            ref_mode,
            ref_options,
            ref_dropdown_open: false,
            ref_selected,
        }
    }
}

impl Default for AddAppForm {
    fn default() -> Self {
        Self::new(Product::new("accounting"))
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
    pub pending_switch_to: Option<Product>,
    pub catalog_notice: Option<String>,
    pub nodes: Vec<Node>,
    pub containers: Vec<Container>,
    pub selected_node: usize,
    pub selected_container: usize,
    pub budget: f64,
    pub telemetry: Box<dyn TelemetryProvider + Send>,
    pub log_scroll: u16,
    pub featured_product: Product,
    pub products: Vec<Product>,
    pub env_states: HashMap<Product, ProductEnv>,
    pub build_env_states: HashMap<Product, ProductEnv>,
    pub env_selected: usize,
    pub build_env_selected: usize,
    pub env_editing: bool,
    pub env_input: String,
    pub env_missing_open: bool,
    pub env_missing_selected: usize,
    pub ops_states: HashMap<Product, OpsStatus>,
    pub ops_selected_action: usize,
    pub preflight: Vec<PreflightCheck>,
    pub preflight_ran: bool,
    pub onboarding_steps: Vec<OnboardingStep>,
    pub show_onboarding: bool,
    pub apps_selected: usize,
    pub doctor: crate::domain::DoctorReport,
    pub doctor_selected: usize,
    /// Drafts/valores informados na criação de novos produtos (acessíveis mid-session, inclusive para reset)
    pub new_product_drafts: HashMap<String, NewProductDraft>,
    pub control_plane: ControlPlane,
}

impl AppState {
    pub fn new(telemetry: Box<dyn TelemetryProvider + Send>) -> Self {
        let discovered = discover_products();
        let catalog_notice = if discovered.is_empty() {
            Some(
                "Nenhum produto encontrado em products/ (modo sem produto configurado)".to_string(),
            )
        } else {
            None
        };
        let products = if discovered.is_empty() {
            vec![Product::new("default")]
        } else {
            discovered
        };
        let featured_product = products[0].clone();
        // Não inventar hosts/containers: o provider popula os dados a partir do
        // Docker no primeiro tick. Antes da coleta a tela fica explicitamente vazia.
        let nodes = Vec::new();
        let containers = Vec::new();
        let mut env_states = HashMap::new();
        let mut build_env_states = HashMap::new();
        for product in &products {
            env_states.insert(product.clone(), load_env_state(product));
            build_env_states.insert(product.clone(), load_build_env_state(product));
        }
        let mut ops_states = HashMap::new();
        for p in &products {
            ops_states.insert(
                p.clone(),
                OpsStatus {
                    product: p.clone(),
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
            pending_switch_to: None,
            catalog_notice,
            nodes,
            containers,
            selected_node: 0,
            selected_container: 0,
            budget: 100.0,
            telemetry,
            log_scroll: 0,
            featured_product,
            products,
            env_states,
            build_env_states,
            env_selected: 0,
            build_env_selected: 0,
            env_editing: false,
            env_input: String::new(),
            env_missing_open: false,
            env_missing_selected: 0,
            ops_states,
            ops_selected_action: 0,
            preflight: Vec::new(),
            preflight_ran: false,
            onboarding_steps: Vec::new(),
            show_onboarding: false,
            apps_selected: 0,
            doctor: crate::domain::DoctorReport::default(),
            doctor_selected: 0,
            new_product_drafts: HashMap::new(),
            control_plane: ControlPlane::new(
                AppRegistry::load(Path::new("apps.yaml")).unwrap_or_else(|_| {
                    AppRegistry::from_yaml("api_version: musical-lamp/v1\nkind: AppRegistry\napps: []").unwrap()
                }),
                StateStore::open(Path::new("deployment.db")).unwrap_or_else(|_| StateStore::open_in_memory().unwrap()),
            ),
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
    pub fn current_build_env(&self) -> Option<&ProductEnv> {
        self.build_env_states.get(&self.featured_product)
    }
    pub fn active_env(&self) -> Option<&ProductEnv> {
        if self.bridge_panel == BridgePanel::BuildEnv {
            self.current_build_env()
        } else {
            self.current_env()
        }
    }
    pub fn active_env_mut(&mut self) -> Option<&mut ProductEnv> {
        if self.bridge_panel == BridgePanel::BuildEnv {
            self.build_env_states.get_mut(&self.featured_product)
        } else {
            self.env_states.get_mut(&self.featured_product)
        }
    }
    pub fn active_env_selected(&self) -> usize {
        if self.bridge_panel == BridgePanel::BuildEnv {
            self.build_env_selected
        } else {
            self.env_selected
        }
    }
    pub fn set_active_env_selected(&mut self, idx: usize) {
        if self.bridge_panel == BridgePanel::BuildEnv {
            self.build_env_selected = idx;
        } else {
            self.env_selected = idx;
        }
    }

    /// Lista variáveis ausentes/problemáticas para navegação rápida no painel Env.
    /// Inclui: chaves obrigatórias vazias/ausentes e chaves com achados de erro.
    pub fn missing_env_vars(&self) -> Vec<String> {
        let Some(env) = self.current_env() else {
            return Vec::new();
        };
        let required = [
            "IMAGE_REF",
            "DB_PASSWORD",
            "TUNNEL_TOKEN",
            "EDGE_SUBNET",
            "EDGE_IP_RANGE",
            "CLOUDFLARED_IP",
        ];
        let mut out: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let var_keys: std::collections::HashSet<String> =
            env.vars.iter().map(|v| v.key.clone()).collect();

        for k in required {
            let missing = match env.vars.iter().find(|v| v.key == k) {
                Some(v) => v.value.trim().is_empty(),
                None => true,
            };
            if missing && seen.insert(k.to_string()) {
                out.push(k.to_string());
            }
        }

        for f in &env.findings {
            // formato: "Error|Warn|Info <KEY|-: msg>" ou mensagens com chave embutida
            let is_error = f.starts_with("Error");
            if !is_error {
                continue;
            }
            // tenta extrair chave após severidade
            let rest = f.split_once(' ').map(|x| x.1).unwrap_or("");
            let key = rest.split(':').next().unwrap_or("").trim();
            if !key.is_empty()
                && key != "-"
                && (var_keys.contains(key) || required.contains(&key))
                && seen.insert(key.to_string())
            {
                out.push(key.to_string());
            }
        }

        // drift: chaves do exemplo ausentes no .env (aparecem nos findings como Warn com essa mensagem)
        for f in &env.findings {
            if f.contains("existe no .env.example e falta no .env") {
                let rest = f.split_once(' ').map(|x| x.1).unwrap_or("");
                let key = rest.split(':').next().unwrap_or("").trim();
                if !key.is_empty() && key != "-" && seen.insert(key.to_string()) {
                    out.push(key.to_string());
                }
            }
        }

        out
    }

    pub fn jump_to_env_key(&mut self, key: &str) {
        let product = self.featured_product.clone();
        let Some(env) = self.env_states.get_mut(&product) else {
            return;
        };
        if let Some(idx) = env.vars.iter().position(|v| v.key == key) {
            self.env_selected = idx;
        } else {
            env.vars.push(crate::domain::EnvVar {
                key: key.to_string(),
                value: String::new(),
                comment: Some("adicionada a partir da lista de variáveis ausentes".into()),
            });
            self.env_selected = env.vars.len().saturating_sub(1);
            env.dirty = true;
            env.last_action = format!("variável ausente adicionada: {key} (preencha e salve)");
        }
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
                "restart" => {
                    ops.state = if success {
                        StackState::Running
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
            Modal::NewProduct(_) => return self.handle_new_product_key(key),
            Modal::ResetEnv(_) => return self.handle_reset_env_key(key),
            Modal::ConfirmText { .. } => return self.handle_confirm_text_key(key),
            Modal::None => {}
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        if self.view == View::BridgesPlugins
            && matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv)
            && self.env_editing
        {
            return self.handle_env_edit_key(key);
        }

        match key.code {
            KeyCode::Char('q') => {
                let dirty_env = self.current_env().map(|e| e.dirty).unwrap_or(false);
                let dirty_build = self.current_build_env().map(|e| e.dirty).unwrap_or(false);
                if dirty_env || dirty_build {
                    let which = match (dirty_env, dirty_build) {
                        (true, true) => ".env e build.env",
                        (true, false) => ".env",
                        (false, true) => "build.env",
                        _ => "arquivos",
                    };
                    self.modal = Modal::ConfirmText {
                        title: "Sair com alterações não salvas".into(),
                        message: format!("O {which} de {} tem alterações não salvas. Digite 'SAIR' para sair sem salvar.", self.featured_product.label()),
                        required: "SAIR".into(),
                        input: String::new(),
                        action: ConfirmAction::QuitApp,
                    };
                    None
                } else {
                    Some(Action::Quit)
                }
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
                }
                None
            }
            KeyCode::Char('N') if self.view == View::ProductLifecycle => {
                // TODO: Implementar lógica de Novo Produto
                None
            }
            KeyCode::Char('S') if self.view == View::ProductLifecycle => {
                // TODO: Implementar lógica de Selecionar Produto
                None
            }
            KeyCode::Char('L') if self.view == View::ProductLifecycle => {
                // TODO: Implementar lógica de Listar Produtos
                None
            }
            KeyCode::Char('I') if self.view == View::ProductLifecycle => {
                // TODO: Implementar lógica de Inspecionar Produto
                None
            }
            KeyCode::Char('R') if self.view == View::ProductLifecycle => {
                // TODO: Implementar lógica de Remover Produto
                None
            }
            KeyCode::Tab => {
                if self.view == View::BridgesPlugins
                    && matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv)
                {
                    self.env_missing_open = false;
                    self.env_editing = false;
                    self.env_input.clear();
                }
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
                        BridgePanel::Env => BridgePanel::BuildEnv,
                        BridgePanel::BuildEnv => BridgePanel::Tunnel,
                        BridgePanel::Tunnel => BridgePanel::Env,
                    };
                    self.clamp_env_selection();
                }
                None
            }
            KeyCode::Char('p') => {
                if let Some(index) = self
                    .products
                    .iter()
                    .position(|product| product == &self.featured_product)
                {
                    let next = (index + 1) % self.products.len();
                    let target = self.products[next].clone();
                    let dirty_env = self.current_env().map(|e| e.dirty).unwrap_or(false);
                    let dirty_build = self.current_build_env().map(|e| e.dirty).unwrap_or(false);
                    if dirty_env || dirty_build {
                        let which = match (dirty_env, dirty_build) {
                            (true, true) => ".env e build.env",
                            (true, false) => ".env",
                            (false, true) => "build.env",
                            _ => "arquivos",
                        };
                        self.pending_switch_to = Some(target.clone());
                        self.modal = Modal::ConfirmText {
                            title: "Alterações não salvas".into(),
                            message: format!("O {which} de {} tem alterações não salvas. Digite '{}' para trocar de produto e descartar salvamento automático (use [w] antes se quiser salvar).", self.featured_product.label(), target.label()),
                            required: target.label().to_string(),
                            input: String::new(),
                            action: ConfirmAction::SwitchProduct(target),
                        };
                    } else {
                        self.switch_to_product(target);
                    }
                }
                if let Modal::AddApp(ref mut form) = self.modal {
                    form.ref_options = Self::ref_options_for_product(&self.featured_product);
                    form.ref_selected = 0;
                    form.ref_dropdown_open = false;
                    if form.ref_mode == RefMode::Dropdown {
                        form.ref_value = form
                            .ref_options
                            .first()
                            .filter(|o| !o.value.is_empty())
                            .map(|o| o.value.clone())
                            .unwrap_or_default();
                        if form.ref_options.is_empty() {
                            form.ref_mode = RefMode::CustomInput;
                        }
                    }
                }
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
            KeyCode::Char('t')
                if self.view == View::ComposeStacks
                    && self.stack_panel == StackPanel::Operations =>
            {
                self.run_ops_action("restart")
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
            KeyCode::Char('m')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                let missing = self.missing_env_vars();
                if missing.is_empty() {
                    self.env_missing_open = false;
                    if let Some(env) = self.current_env_mut() {
                        env.last_action = "nenhuma variável ausente detectada".into();
                    }
                } else {
                    self.env_missing_open = !self.env_missing_open;
                    if self.env_missing_selected >= missing.len() {
                        self.env_missing_selected = 0;
                    }
                }
                None
            }
            KeyCode::Esc
                if self.view == View::BridgesPlugins
                    && self.bridge_panel == BridgePanel::Env
                    && self.env_missing_open =>
            {
                self.env_missing_open = false;
                None
            }
            KeyCode::Enter
                if self.view == View::BridgesPlugins
                    && self.bridge_panel == BridgePanel::Env
                    && self.env_missing_open =>
            {
                let missing = self.missing_env_vars();
                if let Some(key) = missing.get(self.env_missing_selected).cloned() {
                    self.jump_to_env_key(&key);
                    if let Some(env) = self.current_env_mut() {
                        env.last_action = format!("foco na variável ausente: {key}");
                    }
                }
                self.env_missing_open = false;
                None
            }
            KeyCode::Enter
                if self.view == View::BridgesPlugins
                    && matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv)
                    && !self.env_missing_open =>
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
                if self.view == View::BridgesPlugins
                    && matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv) =>
            {
                self.reload_env();
                None
            }
            KeyCode::Char('w')
                if self.view == View::BridgesPlugins
                    && matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv) =>
            {
                self.save_env();
                None
            }
            KeyCode::Char('R')
                if self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env =>
            {
                self.modal = Modal::ResetEnv(ResetEnvForm {
                    product: self.featured_product.clone(),
                    mode: ResetEnvMode::RestoreExample,
                    confirm_input: String::new(),
                });
                None
            }
            KeyCode::Char('N')
                if (self.view == View::BridgesPlugins && self.bridge_panel == BridgePanel::Env)
                    || self.view == View::ComposeStacks =>
            {
                self.open_new_product_modal();
                None
            }
            KeyCode::Char('L') => {
                self.reload_product_catalog();
                if let Some(ops) = self.ops_states.get_mut(&self.featured_product) {
                    ops.last_action = "catálogo de produtos recarregado".into();
                }
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
                if matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv) {
                    if self.bridge_panel == BridgePanel::Env && self.env_missing_open {
                        let len = self.missing_env_vars().len();
                        if len > 0 {
                            self.env_missing_selected = if self.env_missing_selected == 0 {
                                len - 1
                            } else {
                                self.env_missing_selected - 1
                            };
                        }
                    } else {
                        let sel = self.active_env_selected();
                        if sel > 0 {
                            self.set_active_env_selected(sel - 1);
                        }
                    }
                }
            }
            View::ProductLifecycle => {}
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
                    if self.ops_selected_action < 10 {
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
                if matches!(self.bridge_panel, BridgePanel::Env | BridgePanel::BuildEnv) {
                    if self.bridge_panel == BridgePanel::Env && self.env_missing_open {
                        let len = self.missing_env_vars().len();
                        if len > 0 {
                            self.env_missing_selected = (self.env_missing_selected + 1) % len;
                        }
                    } else if let Some(env) = self.active_env() {
                        let sel = self.active_env_selected();
                        if sel + 1 < env.vars.len() {
                            self.set_active_env_selected(sel + 1);
                        }
                    }
                }
            }
            View::ProductLifecycle => {}
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
        if let Some(env) = self.current_build_env() {
            if env.vars.is_empty() {
                self.build_env_selected = 0;
            } else if self.build_env_selected >= env.vars.len() {
                self.build_env_selected = env.vars.len() - 1;
            }
        }
    }

    fn start_env_edit(&mut self) {
        let sel = self.active_env_selected();
        let val = self
            .active_env()
            .and_then(|e| e.vars.get(sel).map(|v| v.value.clone()))
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
                let idx = self.active_env_selected();
                let input = self.env_input.clone();
                if let Some(env) = self.active_env_mut() {
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
        let product = self.featured_product.clone();
        self.generate_env_from_example_for(&product, false);
    }

    fn toggle_env_reveal(&mut self) {
        if let Some(env) = self.active_env_mut() {
            env.reveal_secrets = !env.reveal_secrets;
            env.last_action = if env.reveal_secrets {
                "segredos visíveis".into()
            } else {
                "segredos mascarados".into()
            };
        }
    }

    fn generate_secret_for_selected(&mut self) {
        let idx = self.active_env_selected();
        let Some(env) = self.active_env_mut() else {
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
        let product = self.featured_product.clone();
        if self.bridge_panel == BridgePanel::BuildEnv {
            self.build_env_states
                .insert(product.clone(), load_build_env_state(&product));
        } else {
            self.env_states
                .insert(product.clone(), load_env_state(&product));
        }
        self.clamp_env_selection();
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn save_env(&mut self) {
        let product = self.featured_product.clone();
        let is_build = self.bridge_panel == BridgePanel::BuildEnv;
        let path_s = if is_build {
            product_build_env_path(&product)
        } else {
            product_env_path(&product)
        };
        let path = Path::new(&path_s);
        let content = if let Some(env) = self.active_env() {
            render_env_file(&env.vars)
        } else {
            return;
        };
        // backup antes de gravar se já existe
        if path.exists() || path.is_symlink() {
            let _ = backup_env_file(path);
        }
        let result = write_env_atomic_secure(path, &content);
        if let Some(env) = self.active_env_mut() {
            match result {
                Ok(_) => {
                    env.dirty = false;
                    env.last_action = format!("salvo em {path_s}");
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
        let product = self.featured_product.clone();
        match action {
            "provision_infra" => {
                let plan = self.control_plane.plan(product.label()).unwrap();
                self.control_plane.execute_prepare_infrastructure(product.label(), &crate::executor::LocalExecutor).unwrap();
                return None;
            }
            "provision_db" => {
                let plan = self.control_plane.plan(product.label()).unwrap();
                self.control_plane.execute_database_provisioning(product.label(), &crate::executor::LocalExecutor).unwrap();
                return None;
            }
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
            "down" => {
                self.modal = Modal::ConfirmText {
                    title: "Parar stack".into(),
                    message: format!("Isso executará 'docker compose down' para {} (sem remover volumes). Digite '{}' para confirmar.", product.label(), product.label()),
                    required: product.label().to_string(),
                    input: String::new(),
                    action: ConfirmAction::RunOps { product, action: action.to_string() },
                };
                return None;
            }
            _ => {}
        }

        self.start_ops_action(&product, action)
    }

    fn run_selected_ops_action(&mut self) -> Option<Action> {
        let action = match self.ops_selected_action {
            0 => "up",
            1 => "down",
            2 => "restart",
            3 => "status",
            4 => "provision_infra",
            5 => "provision_db",
            6 => "preflight",
            7 => "doctor",
            8 => "onboarding",
            9 => "migrate",
            _ => "logs",
        };
        self.run_ops_action(action)
    }

    pub fn run_doctor_scan(&mut self, apply_fixes: bool) {
        let product = self.featured_product.clone();
        let label = product.label();
        let env_path_s = product_env_path(&product);
        let example_path_s = product_env_example_path(&product);
        let env_path = Path::new(&env_path_s);
        let example_path = Path::new(&example_path_s);
        let prod_dir = Path::new("products").join(label);

        let mut report = DoctorReport {
            issues: Vec::new(),
            applied: Vec::new(),
            failed: Vec::new(),
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
            let other = other_product(&product)
                .map(|other| product_env_path(&other))
                .and_then(|path| fs::read_to_string(path).ok())
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

            #[cfg(unix)]
            fn resolve_env_target(path: &Path) -> Result<std::path::PathBuf, String> {
                let mut current = path.to_path_buf();
                for _ in 0..40 {
                    let meta = match fs::symlink_metadata(&current) {
                        Ok(m) => m,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(current),
                        Err(e) => return Err(e.to_string()),
                    };
                    if !meta.file_type().is_symlink() {
                        return Ok(current);
                    }
                    let link = fs::read_link(&current).map_err(|e| e.to_string())?;
                    current = if link.is_absolute() {
                        link
                    } else {
                        current
                            .parent()
                            .unwrap_or_else(|| Path::new("."))
                            .join(link)
                    };
                }
                Err("cadeia de links simbólicos muito profunda ou em ciclo".into())
            }

            #[cfg(unix)]
            fn write_env_secure(path: &Path, content: &str) -> Result<(), String> {
                use std::io::Write;
                use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                use std::time::{SystemTime, UNIX_EPOCH};

                // Se .env for link simbólico (inclusive encadeado), gravar no destino final preservando os links.
                let target = resolve_env_target(path)?;

                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let name = target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(".env");
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let tmp = target.with_file_name(format!(".{name}.tmp-{}-{ts}", std::process::id()));
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(&tmp)
                    .map_err(|e| e.to_string())?;
                if let Err(e) = (|| -> Result<(), std::io::Error> {
                    file.write_all(content.as_bytes())?;
                    file.sync_all()?;
                    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
                    Ok(())
                })() {
                    let _ = fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                if let Err(e) = fs::rename(&tmp, &target) {
                    let _ = fs::remove_file(&tmp);
                    return Err(e.to_string());
                }
                // Garantir explicitamente permissões restritas no destino final.
                restrict_env_permissions(&target)
            }

            #[cfg(not(unix))]
            fn resolve_env_target(path: &Path) -> Result<std::path::PathBuf, String> {
                let mut current = path.to_path_buf();
                for _ in 0..40 {
                    let meta = match fs::symlink_metadata(&current) {
                        Ok(m) => m,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(current),
                        Err(e) => return Err(e.to_string()),
                    };
                    if !meta.file_type().is_symlink() {
                        return Ok(current);
                    }
                    let link = fs::read_link(&current).map_err(|e| e.to_string())?;
                    current = if link.is_absolute() {
                        link
                    } else {
                        current
                            .parent()
                            .unwrap_or_else(|| Path::new("."))
                            .join(link)
                    };
                }
                Err("cadeia de links simbólicos muito profunda ou em ciclo".into())
            }

            #[cfg(not(unix))]
            fn write_env_secure(path: &Path, content: &str) -> Result<(), String> {
                use std::time::{SystemTime, UNIX_EPOCH};

                // Se .env for link simbólico (inclusive encadeado), gravar no destino final preservando os links.
                let target = resolve_env_target(path)?;

                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let name = target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(".env");
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let tmp = target.with_file_name(format!(".{name}.tmp-{}-{ts}", std::process::id()));
                fs::write(&tmp, content).map_err(|e| e.to_string())?;
                fs::rename(&tmp, &target).map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    e.to_string()
                })
            }

            let mut pending_applied: Vec<String> = Vec::new();
            let mut created_env = false;
            let mut env_exists_now = env_exists;

            // 1) create env from example (persistido imediatamente e de forma atômica)
            if !env_exists
                && example_exists
                && report
                    .issues
                    .iter()
                    .any(|i| matches!(i.fix, Some(DoctorFixKind::CreateEnvFromExample)))
            {
                match fs::read_to_string(example_path) {
                    Ok(content) => match write_env_secure(env_path, &content) {
                        Ok(_) => {
                            report.applied.push(format!(
                                "Criado products/{label}/.env a partir do .env.example"
                            ));
                            #[cfg(unix)]
                            report
                                .applied
                                .push("Permissões do .env ajustadas para 600".into());
                            created_env = true;
                            env_exists_now = true;
                            self.env_states
                                .insert(product.clone(), load_env_state(&product));
                            working_vars = self
                                .current_env()
                                .map(|e| e.vars.clone())
                                .unwrap_or_default();
                        }
                        Err(e) => {
                            report.failed.push(format!(
                                "Falha ao criar .env com permissões seguras: {e}. Segredos não foram gravados."
                            ));
                        }
                    },
                    Err(e) => {
                        report
                            .failed
                            .push(format!("Falha ao ler .env.example: {e}"));
                    }
                }
            }

            // 2) add missing keys from example (em memória; só marcar aplicado após salvar)
            let mut added = 0usize;
            if example_exists
                && env_exists_now
                && report
                    .issues
                    .iter()
                    .any(|i| matches!(i.fix, Some(DoctorFixKind::AddMissingEnvKeys)))
            {
                if let Ok(ex_c) = fs::read_to_string(example_path) {
                    let ex_vars = parse_env_vars(&ex_c);
                    let mut cur_keys: std::collections::HashSet<String> =
                        working_vars.iter().map(|v| v.key.clone()).collect();
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
                        pending_applied
                            .push(format!("Adicionadas {added} chave(s) do .env.example"));
                    }
                }
            }

            // 3) fill secret/required auto-generatable empties (em memória; só marcar aplicado após salvar)
            let mut filled = 0usize;
            if env_exists_now || !working_vars.is_empty() {
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
                    pending_applied.push(format!(
                        "Preenchidos {filled} segredo(s)/campo(s) automaticamente"
                    ));
                }
            }

            // persist env if we changed in-memory or need normalized save
            let wants_save_normalized = report
                .issues
                .iter()
                .any(|i| matches!(i.fix, Some(DoctorFixKind::SaveEnvNormalized)));
            if !pending_applied.is_empty() || wants_save_normalized {
                let target = Path::new(&env_path_s);
                let existed_before = target.exists();
                // Se o arquivo já existia antes desta execução, só gravar se for legível
                // (para não sobrescrever conteúdo desconhecido em caso de falha parcial).
                let can_write = if existed_before && !created_env {
                    match fs::read_to_string(target) {
                        Ok(_) => true,
                        Err(e) => {
                            report.failed.push(format!(
                                "Falha ao ler .env existente: {e}. Arquivo preservado; nenhuma gravação foi realizada."
                            ));
                            if let Some(env) = self.env_states.get_mut(&product) {
                                env.dirty = false;
                                env.last_action =
                                    "Correção abortada: não foi possível ler o .env existente"
                                        .into();
                            }
                            self.env_states
                                .insert(product.clone(), load_env_state(&product));
                            false
                        }
                    }
                } else if !existed_before && !created_env {
                    // nada a normalizar se não há arquivo e não criamos agora
                    pending_applied.clear();
                    false
                } else {
                    true
                };

                if can_write {
                    let content = render_env_file(&working_vars);
                    match write_env_secure(target, &content) {
                        Ok(_) => {
                            for msg in pending_applied.drain(..) {
                                report.applied.push(msg);
                            }
                            if let Some(env) = self.env_states.get_mut(&product) {
                                env.vars = working_vars.clone();
                                env.dirty = false;
                                env.last_action = "Doctor aplicou correções e salvou o .env".into();
                            } else {
                                self.env_states
                                    .insert(product.clone(), load_env_state(&product));
                            }
                            report.applied.push(format!("Salvo products/{label}/.env"));
                            #[cfg(unix)]
                            report
                                .applied
                                .push("Permissões do .env ajustadas para 600".into());
                        }
                        Err(e) => {
                            report.failed.push(format!(
                                "Falha ao salvar .env com permissões seguras: {e}. Nenhuma alteração pendente foi aplicada."
                            ));
                            // Recarregar do disco para descartar mutações em memória não persistidas.
                            self.env_states
                                .insert(product.clone(), load_env_state(&product));
                            if let Some(env) = self.env_states.get_mut(&product) {
                                env.dirty = false;
                                env.last_action =
                                    "Correção abortada: falha ao gravar o .env".into();
                            }
                        }
                    }
                } else if !pending_applied.is_empty() {
                    // Não foi possível gravar: não registrar correções pendentes como aplicadas.
                    self.env_states
                        .insert(product.clone(), load_env_state(&product));
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
                        .failed
                        .push(format!("Falha ao ajustar permissões do .env: {e}")),
                }
            }

            // reload env state/findings after fixes and rescan for up-to-date report
            self.env_states
                .insert(product.clone(), load_env_state(&product));
            self.refresh_onboarding();

            let applied = report.applied.clone();
            let failed = report.failed.clone();
            self.run_doctor_scan(false);
            self.doctor.applied = applied;
            self.doctor.failed = failed;
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
            self.doctor.summary = match (
                self.doctor.applied.is_empty(),
                self.doctor.failed.is_empty(),
            ) {
                (true, true) => format!(
                    "Doctor: nenhuma correção automática aplicada ({errs} erro(s), {warns} aviso(s))."
                ),
                (false, true) => format!(
                    "Doctor: {} correção(ões) aplicada(s). Estado atual: {errs} erro(s), {warns} aviso(s).",
                    self.doctor.applied.len()
                ),
                (true, false) => format!(
                    "Doctor: {} falha(s) ao aplicar correções. Estado atual: {errs} erro(s), {warns} aviso(s).",
                    self.doctor.failed.len()
                ),
                (false, false) => format!(
                    "Doctor: {} correção(ões) aplicada(s), {} falha(s). Estado atual: {errs} erro(s), {warns} aviso(s).",
                    self.doctor.applied.len(),
                    self.doctor.failed.len()
                ),
            };
            if let Some(ops) = self.ops_states.get_mut(&product) {
                ops.last_action = self.doctor.summary.clone();
                for a in &self.doctor.applied {
                    ops.log_lines.push(format!("[doctor] {a}"));
                }
                for fmsg in &self.doctor.failed {
                    ops.log_lines.push(format!("[doctor][erro] {fmsg}"));
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
            match (report.applied.is_empty(), report.failed.is_empty()) {
                (true, true) => format!(
                    "Doctor: nenhuma correção automática aplicada ({errs} erro(s), {warns} aviso(s))."
                ),
                (false, true) => format!(
                    "Doctor: {} correção(ões) aplicada(s). Restam {errs} erro(s), {warns} aviso(s).",
                    report.applied.len()
                ),
                (true, false) => format!(
                    "Doctor: {} falha(s) ao aplicar correções. Restam {errs} erro(s), {warns} aviso(s).",
                    report.failed.len()
                ),
                (false, false) => format!(
                    "Doctor: {} correção(ões) aplicada(s), {} falha(s). Restam {errs} erro(s), {warns} aviso(s).",
                    report.applied.len(),
                    report.failed.len()
                ),
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
            for fmsg in &report.failed {
                ops.log_lines.push(format!("[doctor][erro] {fmsg}"));
            }
        }
        self.doctor = report;
        if self.doctor_selected >= self.doctor.issues.len() && !self.doctor.issues.is_empty() {
            self.doctor_selected = self.doctor.issues.len() - 1;
        }
    }

    pub fn run_preflight(&mut self) {
        let product = self.featured_product.clone();
        let label = product.label();
        let mut checks = Vec::new();
        let env_path = Path::new("products").join(label).join(".env");
        let example_path = Path::new("products").join(label).join(".env.example");
        let docker = std::process::Command::new("docker")
            .arg("--version")
            .output()
            .is_ok();
        checks.push(if docker {
            PreflightCheck {
                name: "Docker Engine".into(),
                status: PreflightStatus::Ok,
                detail: "docker CLI disponível".into(),
            }
        } else {
            PreflightCheck {
                name: "Docker Engine".into(),
                status: PreflightStatus::Error,
                detail: "docker CLI não encontrado".into(),
            }
        });

        let docker_compose = std::process::Command::new("docker")
            .args(["compose", "version"])
            .output()
            .is_ok();
        checks.push(if docker_compose {
            PreflightCheck {
                name: "Docker Compose".into(),
                status: PreflightStatus::Ok,
                detail: "docker compose disponível".into(),
            }
        } else {
            PreflightCheck {
                name: "Docker Compose".into(),
                status: PreflightStatus::Error,
                detail: "docker compose não encontrado".into(),
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
            let other = other_product(&product)
                .map(|other| product_env_path(&other))
                .and_then(|path| fs::read_to_string(path).ok())
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
        let docker_ok = std::process::Command::new("docker").arg("--version").output().is_ok();
        let docker_compose_ok = std::process::Command::new("docker").args(["compose", "version"]).output().is_ok();
        
        let pre_ok = self.preflight_ran
            && !self
                .preflight
                .iter()
                .any(|c| matches!(c.status, PreflightStatus::Error));

        if self.products.is_empty() {
            self.onboarding_steps = vec![
                OnboardingStep {
                    title: "1. Verificar dependências (Docker/Compose)".into(),
                    done: docker_ok && docker_compose_ok,
                    detail: if docker_ok && docker_compose_ok {
                        "Docker e Docker Compose detectados".into()
                    } else {
                        "Docker CLI ou Docker Compose não encontrados".into()
                    },
                },
                OnboardingStep {
                    title: "2. Configurar domínios e Cloudflare".into(),
                    done: false,
                    detail: "Necessário para expor serviços.".into(),
                },
                OnboardingStep {
                    title: "3. Provisionar ambiente".into(),
                    done: false,
                    detail: "Inicializar volumes e rede.".into(),
                },
            ];
            return;
        }

        let product = self.featured_product.clone();
        let label = product.label();
        let env_path = Path::new("products").join(label).join(".env");
        let example_path = Path::new("products").join(label).join(".env.example");
        let env = self.env_states.get(&product);
        let has_env = env_path.exists();
        let has_example = example_path.exists();
        let tunnel_ok = env_has_tunnel_token(&product);
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
        if !matches!(self.modal, Modal::AddApp(_)) {
            return None;
        }
        match key.code {
            KeyCode::Esc => {
                let should_close = if let Modal::AddApp(ref mut form) = self.modal {
                    if form.field == AppField::Ref && form.ref_dropdown_open {
                        form.ref_dropdown_open = false;
                        false
                    } else {
                        true
                    }
                } else {
                    true
                };
                if should_close {
                    self.modal = Modal::None;
                }
            }
            KeyCode::Tab => {
                if let Modal::AddApp(ref mut form) = self.modal {
                    form.ref_dropdown_open = false;
                    form.field = match form.field {
                        AppField::Url => AppField::Ref,
                        AppField::Ref => AppField::Url,
                    };
                }
            }
            KeyCode::Up => {
                if let Modal::AddApp(ref mut form) = self.modal {
                    if form.field == AppField::Ref
                        && form.ref_mode == RefMode::Dropdown
                        && !form.ref_options.is_empty()
                    {
                        form.ref_dropdown_open = true;
                        if form.ref_selected == 0 {
                            form.ref_selected = form.ref_options.len() - 1;
                        } else {
                            form.ref_selected -= 1;
                        }
                        if let Some(opt) = form.ref_options.get(form.ref_selected) {
                            if !opt.value.is_empty() {
                                form.ref_value = opt.value.clone();
                            }
                        }
                    }
                }
            }
            KeyCode::Down => {
                if let Modal::AddApp(ref mut form) = self.modal {
                    if form.field == AppField::Ref
                        && form.ref_mode == RefMode::Dropdown
                        && !form.ref_options.is_empty()
                    {
                        form.ref_dropdown_open = true;
                        form.ref_selected = (form.ref_selected + 1) % form.ref_options.len();
                        if let Some(opt) = form.ref_options.get(form.ref_selected) {
                            if !opt.value.is_empty() {
                                form.ref_value = opt.value.clone();
                            }
                        }
                    }
                }
            }
            KeyCode::Char('n')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(self.modal, Modal::AddApp(ref f) if f.field == AppField::Ref) =>
            {
                if let Modal::AddApp(ref mut form) = self.modal {
                    form.ref_mode = RefMode::CustomInput;
                    form.ref_dropdown_open = false;
                    form.ref_value.clear();
                    form.error = None;
                }
            }
            KeyCode::Char('l')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(self.modal, Modal::AddApp(ref f) if f.field == AppField::Ref) =>
            {
                if let Modal::AddApp(ref mut form) = self.modal {
                    form.ref_options = AppState::ref_options_for_product(&self.featured_product);
                    form.ref_mode = if form.ref_options.is_empty() {
                        RefMode::CustomInput
                    } else {
                        RefMode::Dropdown
                    };
                    form.ref_selected = 0;
                    form.ref_dropdown_open = true;
                    if let Some(opt) = form.ref_options.first() {
                        if !opt.value.is_empty() {
                            form.ref_value = opt.value.clone();
                        }
                    }
                    form.error = None;
                }
            }
            KeyCode::Backspace => {
                if let Modal::AddApp(ref mut form) = self.modal {
                    match form.field {
                        AppField::Url => {
                            form.url.pop();
                        }
                        AppField::Ref => {
                            if form.ref_mode == RefMode::CustomInput {
                                form.ref_value.pop();
                            } else {
                                // switch to custom input to edit freely
                                form.ref_mode = RefMode::CustomInput;
                                form.ref_dropdown_open = false;
                                form.ref_value.pop();
                            }
                        }
                    }
                    form.error = None;
                }
            }
            KeyCode::Char(' ') if matches!(self.modal, Modal::AddApp(ref f) if f.field == AppField::Ref && f.ref_mode == RefMode::Dropdown) => {
                if let Modal::AddApp(ref mut form) = self.modal {
                    form.ref_dropdown_open = !form.ref_dropdown_open;
                }
            }
            KeyCode::Enter => {
                let mut handled_dropdown = false;
                if let Modal::AddApp(ref mut form) = self.modal {
                    if form.field == AppField::Ref
                        && form.ref_mode == RefMode::Dropdown
                        && form.ref_dropdown_open
                    {
                        if let Some(opt) = form.ref_options.get(form.ref_selected).cloned() {
                            if opt.value.is_empty() {
                                form.ref_mode = RefMode::CustomInput;
                                form.ref_value.clear();
                            } else {
                                form.ref_value = opt.value;
                            }
                            form.ref_dropdown_open = false;
                        }
                        handled_dropdown = true;
                    } else if form.field == AppField::Ref
                        && form.ref_mode == RefMode::Dropdown
                        && !form.ref_dropdown_open
                    {
                        form.ref_dropdown_open = true;
                        handled_dropdown = true;
                    }
                }
                if handled_dropdown {
                    return None;
                }

                let (url, ref_value) = if let Modal::AddApp(ref form) = self.modal {
                    (
                        form.url.trim().to_string(),
                        form.ref_value.trim().to_string(),
                    )
                } else {
                    return None;
                };
                if url.is_empty() || ref_value.is_empty() {
                    if let Modal::AddApp(ref mut form) = self.modal {
                        form.error = Some(
                            "Preencha a URL do repositório e selecione/crie uma ref (tag/commit) vinculada ao produto ativo."
                                .into(),
                        );
                    }
                    return None;
                }
                match self.add_app_entry(&url, &ref_value) {
                    Ok(()) => {
                        self.modal = Modal::None;
                    }
                    Err(e) => {
                        if let Modal::AddApp(ref mut form) = self.modal {
                            form.error = Some(format!("Não foi possível salvar apps.json: {e}"));
                        }
                    }
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Modal::AddApp(ref mut form) = self.modal {
                    match form.field {
                        AppField::Url => form.url.push(c),
                        AppField::Ref => {
                            if form.ref_mode != RefMode::CustomInput {
                                form.ref_mode = RefMode::CustomInput;
                                form.ref_dropdown_open = false;
                            }
                            form.ref_value.push(c);
                        }
                    }
                    form.error = None;
                }
            }
            _ => {}
        }
        None
    }

    fn handle_reset_env_key(&mut self, key: KeyEvent) -> Option<Action> {
        let mut close = false;
        let mut execute: Option<(Product, ResetEnvMode)> = None;
        let has_draft = if let Modal::ResetEnv(ref form) = self.modal {
            self.new_product_drafts.contains_key(form.product.label())
        } else {
            false
        };
        if let Modal::ResetEnv(ref mut form) = self.modal {
            let modes = if has_draft {
                vec![
                    ResetEnvMode::RestoreExample,
                    ResetEnvMode::RemoveEnv,
                    ResetEnvMode::RestoreBackup,
                    ResetEnvMode::RestoreNewProductDraft,
                ]
            } else {
                vec![
                    ResetEnvMode::RestoreExample,
                    ResetEnvMode::RemoveEnv,
                    ResetEnvMode::RestoreBackup,
                ]
            };
            let mut idx = modes.iter().position(|m| m == &form.mode).unwrap_or(0);
            match key.code {
                KeyCode::Esc => close = true,
                KeyCode::Up => {
                    idx = if idx == 0 { modes.len() - 1 } else { idx - 1 };
                    form.mode = modes[idx].clone();
                }
                KeyCode::Down => {
                    idx = (idx + 1) % modes.len();
                    form.mode = modes[idx].clone();
                }
                KeyCode::Backspace => {
                    form.confirm_input.pop();
                }
                KeyCode::Enter => {
                    if form.confirm_input.trim() == form.product.label()
                        || form.confirm_input.trim() == format!("RESET {}", form.product.label())
                    {
                        execute = Some((form.product.clone(), form.mode.clone()));
                        close = true;
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    form.confirm_input.push(c);
                }
                _ => {}
            }
        }
        if close {
            self.modal = Modal::None;
        }
        if let Some((product, mode)) = execute {
            self.reset_env_for_product(&product, mode);
        }
        None
    }

    fn handle_confirm_text_key(&mut self, key: KeyEvent) -> Option<Action> {
        let mut close = false;
        let mut confirmed_action: Option<ConfirmAction> = None;
        if let Modal::ConfirmText {
            required,
            input,
            action,
            ..
        } = &mut self.modal
        {
            match key.code {
                KeyCode::Esc => close = true,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => {
                    if input.trim() == required.trim() {
                        confirmed_action = Some(action.clone());
                        close = true;
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    input.push(c);
                }
                _ => {}
            }
        }
        if close {
            self.modal = Modal::None;
        }
        if let Some(action) = confirmed_action {
            return self.apply_confirm_action(action);
        }
        None
    }

    fn apply_confirm_action(&mut self, action: ConfirmAction) -> Option<Action> {
        match action {
            ConfirmAction::OverwriteEnvFromExample(product) => {
                self.generate_env_from_example_for(&product, true);
                None
            }
            ConfirmAction::QuitApp => Some(Action::Quit),
            ConfirmAction::SwitchProduct(product) => {
                self.switch_to_product(product);
                None
            }
            ConfirmAction::RunOps { product, action } => self.start_ops_action(&product, &action),
        }
    }

    fn start_ops_action(&mut self, product: &Product, action: &str) -> Option<Action> {
        if let Some(cmd_action) = orchestration::build_ops_action(product, action) {
            if let Some(ops) = self.ops_states.get_mut(product) {
                match action {
                    "up" => {
                        ops.state = StackState::Starting;
                        ops.last_action = "subindo a stack (execução real em andamento)".into();
                    }
                    "down" => {
                        ops.state = StackState::Stopping;
                        ops.last_action = "parando a stack (execução real em andamento)".into();
                    }
                    "restart" => {
                        ops.state = StackState::Starting;
                        ops.last_action = "reiniciando a stack (execução real em andamento)".into();
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

    fn generate_env_from_example_for(&mut self, product: &Product, force: bool) {
        let example = product_env_example_path(product);
        let target = product_env_path(product);
        let target_path = Path::new(&target);
        if target_path.exists() && !force {
            self.modal = Modal::ConfirmText {
                title: "Sobrescrever .env".into(),
                message: format!("O arquivo {target} já existe. Digite o nome do produto para sobrescrever a partir do .env.example."),
                required: product.label().to_string(),
                input: String::new(),
                action: ConfirmAction::OverwriteEnvFromExample(product.clone()),
            };
            return;
        }
        match fs::read_to_string(&example) {
            Ok(content) => {
                if target_path.exists() {
                    let _ = backup_env_file(target_path);
                }
                match write_env_atomic_secure(target_path, &content) {
                    Ok(_) => {
                        self.env_states
                            .insert(product.clone(), load_env_state(product));
                        if let Some(env) = self.env_states.get_mut(product) {
                            env.last_action = format!("gerado/sobrescrito a partir de {example}");
                        }
                    }
                    Err(e) => {
                        if let Some(env) = self.env_states.get_mut(product) {
                            env.last_action = format!("falha ao gerar .env: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                if let Some(env) = self.env_states.get_mut(product) {
                    env.last_action = format!("falha ao ler exemplo: {e}");
                }
            }
        }
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn reset_env_for_product(&mut self, product: &Product, mode: ResetEnvMode) {
        let env_path_s = product_env_path(product);
        let env_path = Path::new(&env_path_s);
        let example_path_s = product_env_example_path(product);
        let example_path = Path::new(&example_path_s);
        let result = match mode {
            ResetEnvMode::RestoreExample => {
                if !example_path.exists() {
                    Err(".env.example não encontrado".to_string())
                } else {
                    if env_path.exists() {
                        let _ = backup_env_file(env_path);
                    }
                    fs::read_to_string(example_path)
                        .map_err(|e| e.to_string())
                        .and_then(|c| write_env_atomic_secure(env_path, &c))
                        .map(|_| "restaurado a partir do .env.example".to_string())
                }
            }
            ResetEnvMode::RemoveEnv => {
                if !env_path.exists() {
                    Ok(".env já não existe".to_string())
                } else {
                    let _ = backup_env_file(env_path);
                    fs::remove_file(env_path)
                        .map_err(|e| e.to_string())
                        .map(|_| "arquivo .env removido (backup criado)".to_string())
                }
            }
            ResetEnvMode::RestoreBackup => {
                if let Some(bak) = latest_env_backup(env_path) {
                    if env_path.exists() {
                        let _ = backup_env_file(env_path);
                    }
                    fs::copy(&bak, env_path)
                        .map_err(|e| e.to_string())
                        .map(|_| format!("restaurado de {}", bak.display()))
                } else {
                    Err("nenhum backup .env.bak.* encontrado".to_string())
                }
            }
            ResetEnvMode::RestoreNewProductDraft => {
                self.restore_env_from_new_product_draft(product)
            }
        };
        self.env_states
            .insert(product.clone(), load_env_state(product));
        if let Some(env) = self.env_states.get_mut(product) {
            env.last_action = match result {
                Ok(m) => m,
                Err(e) => format!("reset falhou: {e}"),
            };
            env.dirty = false;
        }
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn restore_env_from_new_product_draft(&mut self, product: &Product) -> Result<String, String> {
        let Some(draft) = self.new_product_drafts.get(product.label()).cloned() else {
            return Err("nenhum rascunho de criação de produto disponível na sessão".into());
        };
        let path_s = product_env_path(product);
        let path = Path::new(&path_s);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if path.exists() || path.is_symlink() {
            let _ = backup_env_file(path);
        }
        let content = build_env_from_draft(&draft);
        write_env_atomic_secure(path, &content).map(|_| {
            "restaurado com os valores informados na criação do produto (sessão)".to_string()
        })
    }

    fn open_new_product_modal(&mut self) {
        let mut draft = NewProductDraft::default();
        let base_id = "novo-produto";
        let mut n = 1;
        let mut candidate = base_id.to_string();
        while self.products.iter().any(|p| p.label() == candidate)
            || Path::new("products").join(&candidate).exists()
        {
            n += 1;
            candidate = format!("{base_id}-{n}");
        }
        draft.id = candidate.clone();
        let octet = 10 + (self.products.len() as u8 % 200);
        draft.edge_subnet = format!("172.30.{octet}.0/28");
        draft.edge_ip_range = format!("172.30.{octet}.8/29");
        draft.cloudflared_ip = format!("172.30.{octet}.2");
        draft.image_ref = format!("kodoo/{candidate}:latest");
        if let Ok(secret) = secrets::generate(secrets::SecretKind::Hex(24)) {
            draft.db_password = secret;
        }
        self.modal = Modal::NewProduct(NewProductForm {
            field: NewProductField::Id,
            draft,
            error: None,
        });
    }

    fn handle_new_product_key(&mut self, key: KeyEvent) -> Option<Action> {
        let mut close = false;
        let mut create = false;
        if let Modal::NewProduct(ref mut form) = self.modal {
            match key.code {
                KeyCode::Esc => close = true,
                KeyCode::Tab => {
                    form.field = match form.field {
                        NewProductField::Id => NewProductField::ImageRef,
                        NewProductField::ImageRef => NewProductField::DbPassword,
                        NewProductField::DbPassword => NewProductField::TunnelToken,
                        NewProductField::TunnelToken => NewProductField::EdgeSubnet,
                        NewProductField::EdgeSubnet => NewProductField::EdgeIpRange,
                        NewProductField::EdgeIpRange => NewProductField::CloudflaredIp,
                        NewProductField::CloudflaredIp => NewProductField::FrappeRef,
                        NewProductField::FrappeRef => NewProductField::AppRepoUrl,
                        NewProductField::AppRepoUrl => NewProductField::AppRepoRef,
                        NewProductField::AppRepoRef => NewProductField::Id,
                    };
                }
                KeyCode::BackTab => {
                    form.field = match form.field {
                        NewProductField::Id => NewProductField::AppRepoRef,
                        NewProductField::ImageRef => NewProductField::Id,
                        NewProductField::DbPassword => NewProductField::ImageRef,
                        NewProductField::TunnelToken => NewProductField::DbPassword,
                        NewProductField::EdgeSubnet => NewProductField::TunnelToken,
                        NewProductField::EdgeIpRange => NewProductField::EdgeSubnet,
                        NewProductField::CloudflaredIp => NewProductField::EdgeIpRange,
                        NewProductField::FrappeRef => NewProductField::CloudflaredIp,
                        NewProductField::AppRepoUrl => NewProductField::FrappeRef,
                        NewProductField::AppRepoRef => NewProductField::AppRepoUrl,
                    };
                }
                KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if form.field == NewProductField::DbPassword {
                        if let Ok(s) = secrets::generate(secrets::SecretKind::Hex(24)) {
                            form.draft.db_password = s;
                        }
                    }
                }
                KeyCode::Backspace => {
                    Self::new_product_field_mut(form).pop();
                    form.error = None;
                }
                KeyCode::Enter => {
                    if let Err(e) = Self::validate_new_product_draft(&form.draft, &self.products) {
                        form.error = Some(e);
                    } else {
                        create = true;
                        close = true;
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Self::new_product_field_mut(form).push(c);
                    form.error = None;
                }
                _ => {}
            }
        }
        let draft = if create {
            if let Modal::NewProduct(ref form) = self.modal {
                Some(form.draft.clone())
            } else {
                None
            }
        } else {
            None
        };
        if close {
            self.modal = Modal::None;
        }
        if let Some(draft) = draft {
            self.create_new_product_from_draft(draft);
        }
        None
    }

    fn new_product_field_mut(form: &mut NewProductForm) -> &mut String {
        match form.field {
            NewProductField::Id => &mut form.draft.id,
            NewProductField::ImageRef => &mut form.draft.image_ref,
            NewProductField::DbPassword => &mut form.draft.db_password,
            NewProductField::TunnelToken => &mut form.draft.tunnel_token,
            NewProductField::EdgeSubnet => &mut form.draft.edge_subnet,
            NewProductField::EdgeIpRange => &mut form.draft.edge_ip_range,
            NewProductField::CloudflaredIp => &mut form.draft.cloudflared_ip,
            NewProductField::FrappeRef => &mut form.draft.frappe_ref,
            NewProductField::AppRepoUrl => &mut form.draft.app_repo_url,
            NewProductField::AppRepoRef => &mut form.draft.app_repo_ref,
        }
    }

    fn validate_new_product_draft(
        draft: &NewProductDraft,
        products: &[Product],
    ) -> Result<(), String> {
        let id = draft.id.trim();
        if id.is_empty() {
            return Err("Informe o id/nome do produto.".into());
        }
        if !Product::is_valid_id(id) {
            return Err(
                "Id inválido: evite vazio, '.', '..', barras e caracteres de controle.".into(),
            );
        }
        if products.iter().any(|p| p.label() == id) || Path::new("products").join(id).exists() {
            return Err(format!("O produto '{id}' já existe."));
        }
        if draft.image_ref.trim().is_empty() {
            return Err("Informe IMAGE_REF.".into());
        }
        Ok(())
    }

    fn create_new_product_from_draft(&mut self, draft: NewProductDraft) {
        let id = draft.id.trim().to_string();
        let dir = Path::new("products").join(&id);
        if let Err(e) = fs::create_dir_all(&dir) {
            self.catalog_notice = Some(format!("Falha ao criar produto '{id}': {e}"));
            return;
        }
        self.new_product_drafts.insert(id.clone(), draft.clone());

        let env_example = build_env_example_from_draft(&draft);
        let env_content = build_env_from_draft(&draft);
        let apps = build_apps_json_from_draft(&draft);
        let build_env = build_build_env_from_draft(&draft);

        let _ = fs::write(dir.join(".env.example"), env_example);
        let _ = fs::write(dir.join("apps.json"), apps);
        let _ = fs::write(dir.join("build.env"), build_env);
        let env_path = dir.join(".env");
        if let Err(e) = write_env_atomic_secure(&env_path, &env_content) {
            let _ = fs::write(&env_path, env_content);
            self.catalog_notice = Some(format!(
                "Produto '{id}' criado, mas houve aviso ao gravar .env com permissões seguras: {e}"
            ));
        } else {
            self.catalog_notice = Some(format!(
                "Produto '{id}' criado e configurado (rascunho salvo na sessão para reset)."
            ));
        }

        self.reload_product_catalog();
        if self.products.iter().any(|p| p.label() == id) {
            self.switch_to_product(Product::new(id));
        }
    }

    pub fn reload_product_catalog(&mut self) {
        let old = self.featured_product.clone();
        let discovered = discover_products();
        self.catalog_notice = if discovered.is_empty() {
            Some(
                "Nenhum produto encontrado em products/ (modo sem produto configurado)".to_string(),
            )
        } else {
            None
        };
        self.products = if discovered.is_empty() {
            vec![Product::new("default")]
        } else {
            discovered
        };
        if !self.products.iter().any(|p| p == &old) {
            self.featured_product = self.products[0].clone();
        }
        for p in &self.products {
            self.env_states
                .entry(p.clone())
                .or_insert_with(|| load_env_state(p));
            self.build_env_states
                .entry(p.clone())
                .or_insert_with(|| load_build_env_state(p));
            self.ops_states
                .entry(p.clone())
                .or_insert_with(|| OpsStatus {
                    product: p.clone(),
                    state: StackState::Stopped,
                    last_action: "produto carregado".into(),
                    log_lines: vec![],
                });
        }
        self.refresh_onboarding();
        self.run_doctor_scan(false);
    }

    fn switch_to_product(&mut self, product: Product) {
        self.featured_product = product;
        self.clamp_env_selection();
        self.preflight.clear();
        self.preflight_ran = false;
        self.refresh_onboarding();
        self.run_doctor_scan(false);
        self.apps_selected = 0;
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

    fn is_placeholder_ref(value: &str) -> bool {
        let v = value.trim();
        v.is_empty()
            || (v.starts_with('<') && v.ends_with('>'))
            || v.contains("TAG_OU_COMMIT")
            || v.contains("TAG_DO_")
            || v.contains("BRANCH")
            || v.contains("PLACEHOLDER")
    }

    pub fn ref_options_for_product(product: &Product) -> Vec<RefOption> {
        let mut options: Vec<RefOption> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let path = Path::new("products")
            .join(product.label())
            .join("apps.json");
        let arr = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
            Err(_) => vec![],
        };

        for a in &arr {
            if let Some(b) = a.get("branch").and_then(|x| x.as_str()) {
                let val = b.to_string();
                if seen.insert(format!("v:{val}")) {
                    if Self::is_placeholder_ref(&val) {
                        options.push(RefOption {
                            label: format!("Aguardada: {val}"),
                            value: val,
                            awaited: true,
                        });
                    } else {
                        options.push(RefOption {
                            label: format!("Existente: {val}"),
                            value: val,
                            awaited: false,
                        });
                    }
                }
            }
        }

        let build_path = Path::new("products")
            .join(product.label())
            .join("build.env");
        if let Ok(content) = fs::read_to_string(build_path) {
            for line in content.lines() {
                let t = line.trim();
                if let Some(v) = t.strip_prefix("FRAPPE_REF=") {
                    let val = v.trim().to_string();
                    if !val.is_empty()
                        && Self::is_placeholder_ref(&val)
                        && seen.insert(format!("b:{val}"))
                    {
                        options.push(RefOption {
                            label: format!("Aguardada (build): {val}"),
                            value: val,
                            awaited: true,
                        });
                    } else if val.is_empty() && seen.insert("b:FRAPPE_REF_EMPTY".to_string()) {
                        options.push(RefOption {
                            label: "Aguardada (build): definir FRAPPE_REF".into(),
                            value: String::new(),
                            awaited: true,
                        });
                    }
                }
            }
        }

        options.sort_by(|a, b| a.awaited.cmp(&b.awaited).then(a.label.cmp(&b.label)));
        options.push(RefOption {
            label: "Criar nova ref (digitar)".into(),
            value: String::new(),
            awaited: false,
        });
        options
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
        // backup (ultimos 5) + escrita atomica: um 'd' sem querer ou uma queda no meio nao perde o manifesto
        if path.exists() {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = fs::copy(path, path.with_file_name(format!("apps.json.bak-{ts}")));
            if let Some(dir) = path.parent() {
                let mut baks: Vec<String> = fs::read_dir(dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .filter_map(|e| e.file_name().into_string().ok())
                            .filter(|n| n.starts_with("apps.json.bak-"))
                            .collect()
                    })
                    .unwrap_or_default();
                baks.sort();
                while baks.len() > 5 {
                    let _ = fs::remove_file(dir.join(baks.remove(0)));
                }
            }
        }
        let tmp = path.with_file_name("apps.json.tmp");
        fs::write(&tmp, out + "\n")
            .map_err(|e| format!("falha ao salvar {}: {e}", tmp.display()))?;
        fs::rename(&tmp, path).map_err(|e| format!("falha ao salvar {}: {e}", path.display()))
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

fn build_env_from_draft(draft: &NewProductDraft) -> String {
    let id = draft.id.trim();
    format!(
        "# Copie para .env e preencha. chmod 600 .env. Nunca versione o .env.\n\
\n\
# --- Imagem -------------------------------------------------------------------\n\
# Em producao use o DIGEST: kodoo/{id}@sha256:...\n\
IMAGE_REF={}\n\
PULL_POLICY=missing\n\
\n\
# --- Banco / cache ------------------------------------------------------------\n\
# Gere com: openssl rand -hex 24\n\
DB_PASSWORD={}\n\
MARIADB_IMAGE=mariadb:11.8\n\
# Fixe tag (ou digest) em producao.\n\
REDIS_IMAGE=redis:8-alpine\n\
\n\
# --- Frappe -------------------------------------------------------------------\n\
# Vazio = multi-tenant (site escolhido pelo Host). Preenchido = single-site.\n\
FRAPPE_SITE_NAME_HEADER=\n\
CLIENT_MAX_BODY_SIZE=50m\n\
# Caminho do Chromium dentro da imagem (confirme: dpkg -L chromium-headless-shell).\n\
# Vazio = nao configura.\n\
CHROMIUM_PATH=\n\
\n\
# --- Cloudflare Tunnel (um conector por produto) ------------------------------\n\
# Token do túnel (Cloudflare Zero Trust).\n\
TUNNEL_TOKEN={}\n\
# Rede de borda exclusiva (cloudflared + frontend). Nao pode colidir entre produtos.\n\
EDGE_SUBNET={}\n\
# Faixa de IPs dinamicos (metade superior da sub-rede); deve NAO conter o CLOUDFLARED_IP.\n\
EDGE_IP_RANGE={}\n\
# IP fixo do cloudflared nessa rede; e o unico em que o nginx confia para CF-Connecting-IP.\n\
CLOUDFLARED_IP={}\n",
        draft.image_ref.trim(),
        draft.db_password.trim(),
        draft.tunnel_token.trim(),
        draft.edge_subnet.trim(),
        draft.edge_ip_range.trim(),
        draft.cloudflared_ip.trim(),
    )
}

fn build_env_example_from_draft(draft: &NewProductDraft) -> String {
    let mut d = draft.clone();
    d.db_password.clear();
    d.tunnel_token.clear();
    build_env_from_draft(&d)
}

fn build_apps_json_from_draft(draft: &NewProductDraft) -> String {
    let url = draft.app_repo_url.trim();
    let refv = draft.app_repo_ref.trim();
    if url.is_empty() {
        return "[]\n".to_string();
    }
    serde_json::to_string_pretty(&serde_json::json!([
        {
            "url": url,
            "branch": if refv.is_empty() { "<TAG_OU_COMMIT>" } else { refv }
        }
    ]))
    .unwrap_or_else(|_| "[]\n".to_string())
        + "\n"
}

fn build_build_env_from_draft(draft: &NewProductDraft) -> String {
    let id = draft.id.trim();
    let frappe_ref = draft.frappe_ref.trim();
    format!(
        "IMAGE=kodoo/{id}\n\
IMAGE_TITLE=kodoo-{id}\n\
# Tag (ou commit) do Frappe Framework. Obrigatorio.\n\
FRAPPE_REF={}\n\
# Mantido ate converter os print formats legados do ERPNext para Chromium.\n\
INSTALL_WKHTMLTOPDF=true\n\
INSTALL_CHROMIUM=true\n",
        if frappe_ref.is_empty() {
            "<TAG_OU_COMMIT_FRAPPE>"
        } else {
            frappe_ref
        }
    )
}

pub fn product_env_path(product: &Product) -> String {
    format!("products/{}/.env", product.label())
}
pub fn product_env_example_path(product: &Product) -> String {
    format!("products/{}/.env.example", product.label())
}
pub fn product_build_env_path(product: &Product) -> String {
    format!("products/{}/build.env", product.label())
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

pub fn load_env_state(product: &Product) -> ProductEnv {
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
        let other_env = other_product(product)
            .map(|other| product_env_path(&other))
            .and_then(|path| fs::read_to_string(path).ok())
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
        product: product.clone(),
        path_hint,
        vars,
        dirty: false,
        last_action,
        reveal_secrets: false,
        findings,
    }
}

pub fn load_build_env_state(product: &Product) -> ProductEnv {
    let path_s = product_build_env_path(product);
    let path = Path::new(&path_s);
    let (path_hint, content, last_action) = if path.exists() {
        match fs::read_to_string(path) {
            Ok(c) => (path_s.clone(), c, format!("carregado de {path_s}")),
            Err(e) => (
                path_s.clone(),
                String::new(),
                format!("erro ao ler {path_s}: {e}"),
            ),
        }
    } else {
        (
            path_s.clone(),
            String::new(),
            "nenhum build.env encontrado".into(),
        )
    };
    let vars = if content.is_empty() {
        Vec::new()
    } else {
        parse_env_vars(&content)
    };
    let mut findings = Vec::new();
    let prod_dir = Path::new("products").join(product.label());
    for fnd in validate::validate_build(&prod_dir) {
        findings.push(format!(
            "{:?} {}: {}",
            fnd.severity,
            fnd.key.unwrap_or_else(|| "-".into()),
            fnd.message
        ));
    }
    if vars.is_empty() && path.exists() {
        findings.push("Info -: build.env vazio".into());
    }
    if !path.exists() {
        findings.push("Warn -: build.env ausente (opcional conforme produto, mas recomendado para build da imagem)".into());
    }
    ProductEnv {
        product: product.clone(),
        path_hint,
        vars,
        dirty: false,
        last_action,
        reveal_secrets: false,
        findings,
    }
}

pub fn env_has_tunnel_token(product: &Product) -> bool {
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

pub fn backup_env_file(path: &Path) -> Result<std::path::PathBuf, String> {
    if !path.exists() && !path.is_symlink() {
        return Err("arquivo inexistente".into());
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(".env");
    let bak = path.with_file_name(format!("{name}.bak.{ts}"));
    fs::copy(path, &bak).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&bak, fs::Permissions::from_mode(0o600));
    }
    Ok(bak)
}

pub fn latest_env_backup(path: &Path) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let prefix = format!("{name}.bak.");
    let mut backups: Vec<_> = fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect();
    backups.sort();
    backups.pop()
}

pub fn write_env_atomic_secure(path: &Path, content: &str) -> Result<(), String> {
    // resolve symlink chain to final target (reuse logic similar to ops_core)
    let mut target = path.to_path_buf();
    for _ in 0..40 {
        let meta = match fs::symlink_metadata(&target) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.to_string()),
        };
        if !meta.file_type().is_symlink() {
            break;
        }
        let link = fs::read_link(&target).map_err(|e| e.to_string())?;
        target = if link.is_absolute() {
            link
        } else {
            target.parent().unwrap_or_else(|| Path::new(".")).join(link)
        };
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".env");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = target.with_file_name(format!(".{name}.tmp-{}-{ts}", std::process::id()));

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        if let Err(e) = (|| -> Result<(), std::io::Error> {
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
            Ok(())
        })() {
            let _ = fs::remove_file(&tmp);
            return Err(e.to_string());
        }
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp, content).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod doctor_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    struct CwdGuard {
        prev: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev);
        }
    }

    fn enter_temp_workspace(prefix: &str) -> (CwdGuard, PathBuf) {
        let lock = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::current_dir().expect("cwd");
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "musical-lamp-{prefix}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("products/accounting")).unwrap();
        fs::create_dir_all(dir.join("products/trivia")).unwrap();
        fs::create_dir_all(dir.join("common")).unwrap();
        // minimal files so AppState preflight/onboarding don't panic
        fs::write(dir.join("common/compose.base.yaml"), "services: {}\n").unwrap();
        fs::write(dir.join("common/dc.sh"), "#!/bin/sh\nexit 0\n").unwrap();
        std::env::set_current_dir(&dir).unwrap();
        (CwdGuard { prev, _lock: lock }, dir)
    }

    #[test]
    fn failed_write_preserves_existing_env_and_does_not_mark_applied() {
        let (_guard, dir) = enter_temp_workspace("doctor-fail");
        let env_path = dir.join("products/accounting/.env");
        let example_path = dir.join("products/accounting/.env.example");
        // existing env missing DB_PASSWORD (auto-fixable) so doctor will try to fill and save
        let original = "IMAGE_REF=ghcr.io/example/app:1.0.0\nDB_PASSWORD=\nTUNNEL_TOKEN=tok12345678901234567890\nEDGE_SUBNET=172.30.0.0/24\nEDGE_IP_RANGE=172.30.0.0/24\nCLOUDFLARED_IP=172.30.0.2\n";
        fs::write(&env_path, original).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&env_path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        fs::write(&example_path, original).unwrap();

        let telemetry: Box<dyn crate::telemetry::TelemetryProvider + Send> =
            Box::new(crate::telemetry::MockTelemetryProvider::new());
        let mut state = AppState::new(telemetry);
        state.featured_product = Product::new("accounting");
        state.env_states.insert(
            Product::new("accounting"),
            load_env_state(&Product::new("accounting")),
        );
        state.run_doctor_scan(false);

        // force write failure without relying on permissions root may bypass:
        // replace .env by a symlink to a directory so atomic rename of the temp file fails.
        let target_dir = dir.join("products/accounting/env-target-dir");
        fs::create_dir_all(&target_dir).unwrap();
        fs::remove_file(&env_path).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target_dir, &env_path).unwrap();
        #[cfg(not(unix))]
        {
            // fallback: make destination a directory to force rename failure
            fs::create_dir_all(&env_path).unwrap();
        }

        state.run_doctor_scan(true);

        #[cfg(unix)]
        {
            let meta = fs::symlink_metadata(&env_path).unwrap();
            assert!(
                meta.file_type().is_symlink(),
                "falha de gravação não deve substituir link simbólico"
            );
            assert!(
                target_dir.is_dir(),
                "destino do link deve permanecer intacto"
            );
        }
        #[cfg(not(unix))]
        {
            assert!(
                env_path.is_dir(),
                "falha de gravação não deve corromper o destino existente"
            );
        }

        let has_fill_applied = state.doctor.applied.iter().any(|m| {
            m.contains("Preenchidos")
                || m.contains("Adicionadas")
                || m.contains("Salvo products/accounting/.env")
        });
        assert!(
            !has_fill_applied,
            "correções não persistidas não devem constar como aplicadas: {:?} (falhas: {:?})",
            state.doctor.applied, state.doctor.failed
        );
        assert!(
            !state.doctor.failed.is_empty(),
            "falha de gravação deve ser reportada"
        );
    }

    #[cfg(unix)]
    #[test]
    fn chained_symlink_env_writes_to_final_target_and_preserves_links() {
        use std::os::unix::fs::symlink;

        let (_guard, dir) = enter_temp_workspace("doctor-chain");
        let env_path = dir.join("products/accounting/.env");
        let example_path = dir.join("products/accounting/.env.example");
        let intermediate = dir.join("products/accounting/.env.link");
        let real = dir.join("products/accounting/.env.real");
        let original = "IMAGE_REF=ghcr.io/example/app:1.0.0\nDB_PASSWORD=\nTUNNEL_TOKEN=tok12345678901234567890\nEDGE_SUBNET=172.30.0.0/24\nEDGE_IP_RANGE=172.30.0.0/24\nCLOUDFLARED_IP=172.30.0.2\n";
        fs::write(&real, original).unwrap();
        fs::write(&example_path, original).unwrap();
        symlink(&real, &intermediate).unwrap();
        symlink(" .env.link".trim(), &env_path).unwrap(); // relative intermediate link

        let telemetry: Box<dyn crate::telemetry::TelemetryProvider + Send> =
            Box::new(crate::telemetry::MockTelemetryProvider::new());
        let mut state = AppState::new(telemetry);
        state.featured_product = Product::new("accounting");
        state.env_states.insert(
            Product::new("accounting"),
            load_env_state(&Product::new("accounting")),
        );
        state.run_doctor_scan(true);

        let env_meta = fs::symlink_metadata(&env_path).unwrap();
        assert!(
            env_meta.file_type().is_symlink(),
            ".env deve permanecer como link simbólico"
        );
        let inter_meta = fs::symlink_metadata(&intermediate).unwrap();
        assert!(
            inter_meta.file_type().is_symlink(),
            "link intermediário deve permanecer"
        );
        let final_content = fs::read_to_string(&real).unwrap();
        assert!(
            final_content.contains("IMAGE_REF="),
            "arquivo final deve ser atualizado"
        );
        assert!(
            !final_content.contains("DB_PASSWORD=\n"),
            "correção automática deve preencher senha no destino final"
        );
        assert!(
            state.doctor.failed.is_empty(),
            "gravação em cadeia de links deve ter sucesso: {:?}",
            state.doctor.failed
        );
    }

    #[test]
    fn unreadable_existing_env_is_preserved_and_not_marked_applied() {
        // Se o .env existente não pode ser lido, o Doctor não deve gravar por cima nem marcar correções como aplicadas.
        let (_guard, dir) = enter_temp_workspace("doctor-unreadable");
        let env_path = dir.join("products/accounting/.env");
        let example_path = dir.join("products/accounting/.env.example");
        let original = "IMAGE_REF=\nDB_PASSWORD=\nTUNNEL_TOKEN=tok12345678901234567890\nEDGE_SUBNET=172.30.0.0/24\nEDGE_IP_RANGE=172.30.0.0/24\nCLOUDFLARED_IP=172.30.0.2\n";
        fs::write(&env_path, original).unwrap();
        fs::write(&example_path, original).unwrap();

        // tornar ilegível sem depender de permissões que root pode contornar:
        // substituir o arquivo por um diretório (read_to_string falha com EISDIR).
        fs::remove_file(&env_path).unwrap();
        fs::create_dir(&env_path).unwrap();

        let telemetry: Box<dyn crate::telemetry::TelemetryProvider + Send> =
            Box::new(crate::telemetry::MockTelemetryProvider::new());
        let mut state = AppState::new(telemetry);
        state.featured_product = Product::new("accounting");
        state.env_states.insert(
            Product::new("accounting"),
            load_env_state(&Product::new("accounting")),
        );
        state.run_doctor_scan(true);

        assert!(
            env_path.is_dir(),
            ".env existente não legível como arquivo deve ser preservado sem ser substituído"
        );
        let has_saved = state
            .doctor
            .applied
            .iter()
            .any(|m| m.contains("Salvo products/accounting/.env") || m.contains("Preenchidos"));
        assert!(
            !has_saved,
            "não deve marcar como aplicado quando não foi possível ler/gravar: {:?}",
            state.doctor.applied
        );
        assert!(
            state
                .doctor
                .failed
                .iter()
                .any(|m| m.contains("ler .env existente") || m.contains("Falha")),
            "deve reportar falha ao ler .env existente: {:?}",
            state.doctor.failed
        );
    }
}
