use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, Event};
use crate::domain::{allocate_costs, Container, Node};
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

    pub fn handle_event(&mut self, event: Event) -> Option<Action> {
        match event {
            Event::Tick => {
                self.telemetry.tick(&mut self.nodes, &mut self.containers);
                None
            }
            Event::Resize => None,
            Event::Key(key) => self.handle_key(key),
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
        match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('1') => {
                if self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::Logs;
                } else {
                    self.view = View::ClusterFinOps;
                }
                None
            }
            KeyCode::Char('2') => {
                if self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::CostBreakdown;
                } else {
                    self.view = View::ComposeStacks;
                }
                None
            }
            KeyCode::Char('3') => {
                if self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::ComposeYaml;
                } else {
                    self.view = View::BridgesPlugins;
                }
                None
            }
            KeyCode::Char('4') => {
                if self.focus == FocusPane::Detail {
                    self.detail_tab = DetailTab::Forecast7d;
                }
                None
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Nodes => FocusPane::Containers,
                    FocusPane::Containers => FocusPane::Detail,
                    FocusPane::Detail => FocusPane::Nodes,
                };
                None
            }
            KeyCode::Char('l') if self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::Logs; None }
            KeyCode::Char('b') if self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::CostBreakdown; None }
            KeyCode::Char('y') if self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::ComposeYaml; None }
            KeyCode::Char('f') if self.focus == FocusPane::Detail => { self.detail_tab = DetailTab::Forecast7d; None }
            KeyCode::Up => { self.move_up(); None }
            KeyCode::Down => { self.move_down(); None }
            KeyCode::Char('a') => { self.modal = Modal::AddNode(AddNodeForm::default()); None }
            KeyCode::Char('d') => {
                if !self.nodes.is_empty() {
                    self.modal = Modal::DropNode { confirm: false };
                }
                None
            }
            KeyCode::Char('s') => { self.simulate_load(); None }
            _ => None,
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            FocusPane::Nodes => {
                if self.selected_node > 0 { self.selected_node -= 1; self.selected_container = 0; }
            }
            FocusPane::Containers => {
                if self.selected_container > 0 { self.selected_container -= 1; }
            }
            FocusPane::Detail => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
        }
    }
    fn move_down(&mut self) {
        match self.focus {
            FocusPane::Nodes => {
                if self.selected_node + 1 < self.nodes.len() { self.selected_node += 1; self.selected_container = 0; }
            }
            FocusPane::Containers => {
                let len = self.containers_for_selected_node().len().saturating_add(1); // +idle row
                if self.selected_container + 1 < len { self.selected_container += 1; }
            }
            FocusPane::Detail => {
                self.log_scroll = self.log_scroll.saturating_add(1);
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
            KeyCode::Backspace => {
                let s = Self::active_field_mut(form);
                s.pop();
            }
            KeyCode::Enter => {
                if let Ok(cost) = form.cost.trim().parse::<f64>() {
                    let idx = self.nodes.len() + 1;
                    let name = if form.name.trim().is_empty() { format!("node-{idx}") } else { form.name.trim().to_string() };
                    let region = "custom".to_string();
                    self.nodes.push(Node {
                        id: format!("n{idx}"),
                        name: name.clone(),
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
            KeyCode::Char(_) => {}
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
            KeyCode::Char('y') => {
                if let Modal::DropNode { ref mut confirm } = self.modal { *confirm = true; }
            }
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
}
