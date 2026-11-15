//! Telemetria local do Docker sem bloquear o loop visual da TUI.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::domain::{Container, Node, Product};

pub trait TelemetryProvider {
    fn tick(&mut self, nodes: &mut Vec<Node>, containers: &mut Vec<Container>);
}

#[derive(Clone, Debug, Default)]
pub struct TelemetryMeta {
    #[allow(dead_code)]
    pub last_success: Option<Instant>,
    pub last_error: Option<String>,
    pub docker_available: bool,
}

#[derive(Clone)]
struct Snapshot {
    nodes: Vec<Node>,
    containers: Vec<Container>,
    meta: TelemetryMeta,
}

#[derive(Clone)]
pub struct DockerTelemetryProvider {
    cache: Arc<Mutex<Option<Snapshot>>>,
    stop: Arc<AtomicBool>,
}

impl DockerTelemetryProvider {
    pub fn new(products: Vec<Product>) -> Self {
        Self::with_interval(products, Duration::from_secs(5))
    }

    pub fn with_interval(products: Vec<Product>, interval: Duration) -> Self {
        let interval = interval.max(Duration::from_secs(2));
        let cache = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_cache = Arc::clone(&cache);
        let worker_stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("docker-telemetry".into())
            .spawn(move || {
                let mut backoff = Duration::from_secs(0);
                let mut history: HashMap<String, (VecDeque<u64>, VecDeque<u64>)> = HashMap::new();
                loop {
                    if worker_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let products_now = if products.is_empty() {
                        crate::domain::discover_products()
                    } else {
                        products.clone()
                    };
                    match collect(&products_now, &mut history) {
                        Ok(mut snapshot) => {
                            if let Ok(prev) = worker_cache.lock() {
                                if let Some(prev) = prev.as_ref() {
                                    // preservar histórico mais longo se já tínhamos nós
                                    merge_histories(&mut snapshot, prev);
                                }
                            }
                            if let Ok(mut cached) = worker_cache.lock() {
                                *cached = Some(snapshot);
                            }
                            backoff = Duration::from_secs(0);
                        }
                        Err(err) => {
                            if let Ok(mut cached) = worker_cache.lock() {
                                if let Some(prev) = cached.as_mut() {
                                    prev.meta.last_error = Some(err.clone());
                                    prev.meta.docker_available = false;
                                } else {
                                    *cached = Some(Snapshot {
                                        nodes: Vec::new(),
                                        containers: Vec::new(),
                                        meta: TelemetryMeta {
                                            last_success: None,
                                            last_error: Some(err),
                                            docker_available: false,
                                        },
                                    });
                                }
                            }
                            backoff = if backoff.is_zero() {
                                Duration::from_secs(5)
                            } else {
                                (backoff * 2).min(Duration::from_secs(30))
                            };
                        }
                    }
                    let sleep_for = if backoff.is_zero() { interval } else { backoff };
                    let start = Instant::now();
                    while start.elapsed() < sleep_for {
                        if worker_stop.load(Ordering::Relaxed) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(200));
                    }
                }
            })
            .expect("não foi possível iniciar a coleta de telemetria Docker");
        Self { cache, stop }
    }

    #[allow(dead_code)]
    pub fn snapshot(&self) -> Option<(Vec<Node>, Vec<Container>, TelemetryMeta)> {
        self.cache.lock().ok().and_then(|c| {
            c.as_ref()
                .map(|s| (s.nodes.clone(), s.containers.clone(), s.meta.clone()))
        })
    }
}

impl Drop for DockerTelemetryProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Default for DockerTelemetryProvider {
    fn default() -> Self {
        Self::new(crate::domain::discover_products())
    }
}

impl TelemetryProvider for DockerTelemetryProvider {
    fn tick(&mut self, nodes: &mut Vec<Node>, containers: &mut Vec<Container>) {
        let snapshot = self.cache.lock().ok().and_then(|cached| cached.clone());
        if let Some(snapshot) = snapshot {
            *nodes = snapshot.nodes;
            *containers = snapshot.containers;
        }
    }
}

fn merge_histories(new_snapshot: &mut Snapshot, prev: &Snapshot) {
    let mut prev_nodes: HashMap<String, &Node> = HashMap::new();
    for n in &prev.nodes {
        prev_nodes.insert(n.id.clone(), n);
    }
    for n in &mut new_snapshot.nodes {
        if let Some(old) = prev_nodes.get(&n.id) {
            let mut cpu: Vec<u64> = old.cpu_history.clone();
            if let Some(last) = n.cpu_history.last() {
                cpu.push(*last);
            }
            let mut ram: Vec<u64> = old.ram_history.clone();
            if let Some(last) = n.ram_history.last() {
                ram.push(*last);
            }
            if cpu.len() > 30 {
                cpu.drain(0..cpu.len() - 30);
            }
            if ram.len() > 30 {
                ram.drain(0..ram.len() - 30);
            }
            n.cpu_history = cpu;
            n.ram_history = ram;
        }
    }
}

/// Mock realista para demonstração: cria um pool multi-servidor com flutuação orgânica
/// de CPU/RAM e recálculo dinâmico de custos proporcionais a cada tick (~1s).
#[derive(Debug, Clone)]
pub struct MockTelemetryProvider {
    initialized: bool,
    t: u64,
}

impl Default for MockTelemetryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTelemetryProvider {
    pub fn new() -> Self {
        Self {
            initialized: false,
            t: 0,
        }
    }

    fn seed(nodes: &mut Vec<Node>, containers: &mut Vec<Container>) {
        if !nodes.is_empty() || !containers.is_empty() {
            return;
        }
        *nodes = vec![
            Node {
                id: "node-hetzner-hel1".into(),
                name: "hetzner-hel1".into(),
                region: "eu".into(),
                host: "10.0.0.11".into(),
                ssh_user: "root".into(),
                provider: "Hetzner".into(),
                monthly_cost: 13.99,
                online: true,
                cpu_pct: 38.0,
                ram_pct: 46.0,
                cpu_history: vec![32, 34, 36, 35, 37, 38],
                ram_history: vec![44, 45, 45, 46, 46, 46],
            },
            Node {
                id: "node-aws-use1".into(),
                name: "aws-use1".into(),
                region: "us-east-1".into(),
                host: "10.0.1.20".into(),
                ssh_user: "ubuntu".into(),
                provider: "AWS".into(),
                monthly_cost: 28.40,
                online: true,
                cpu_pct: 61.0,
                ram_pct: 54.0,
                cpu_history: vec![55, 57, 58, 60, 59, 61],
                ram_history: vec![50, 51, 52, 53, 53, 54],
            },
            Node {
                id: "node-do-nyc1".into(),
                name: "do-nyc1".into(),
                region: "nyc1".into(),
                host: "10.0.2.7".into(),
                ssh_user: "root".into(),
                provider: "DigitalOcean".into(),
                monthly_cost: 24.00,
                online: true,
                cpu_pct: 22.0,
                ram_pct: 31.0,
                cpu_history: vec![18, 19, 21, 20, 22, 22],
                ram_history: vec![28, 29, 30, 30, 31, 31],
            },
        ];
        *containers = vec![
            mk_container(
                "c-hel1-api",
                "api-gateway",
                "node-hetzner-hel1",
                "edge",
                34.0,
                28.0,
            ),
            mk_container(
                "c-hel1-web",
                "web-frontend",
                "node-hetzner-hel1",
                "edge",
                18.0,
                22.0,
            ),
            mk_container(
                "c-hel1-worker",
                "jobs-worker",
                "node-hetzner-hel1",
                "edge",
                12.0,
                16.0,
            ),
            mk_container(
                "c-aws-db",
                "postgres-primary",
                "node-aws-use1",
                "data",
                30.0,
                44.0,
            ),
            mk_container(
                "c-aws-api",
                "billing-api",
                "node-aws-use1",
                "billing",
                26.0,
                24.0,
            ),
            mk_container(
                "c-aws-cache",
                "redis-cache",
                "node-aws-use1",
                "data",
                10.0,
                18.0,
            ),
            mk_container(
                "c-do-analytics",
                "analytics",
                "node-do-nyc1",
                "insights",
                14.0,
                20.0,
            ),
            mk_container(
                "c-do-cron",
                "scheduler",
                "node-do-nyc1",
                "insights",
                6.0,
                8.0,
            ),
        ];
        for c in containers.iter_mut() {
            c.logs = sample_logs(&c.name);
        }
    }

    fn organic(base: f64, phase: f64, amp: f64, drift: f64) -> f64 {
        let wave = (phase).sin() * amp + ((phase * 0.7) + 1.3).cos() * (amp * 0.45);
        (base + wave + drift).clamp(2.0, 97.0)
    }

    fn push_hist(hist: &mut Vec<u64>, v: f64) {
        hist.push(v.round().clamp(0.0, 100.0) as u64);
        if hist.len() > 30 {
            let drop = hist.len() - 30;
            hist.drain(0..drop);
        }
    }
}

impl TelemetryProvider for MockTelemetryProvider {
    fn tick(&mut self, nodes: &mut Vec<Node>, containers: &mut Vec<Container>) {
        if !self.initialized {
            Self::seed(nodes, containers);
            self.initialized = true;
        }
        self.t = self.t.wrapping_add(1);
        let t = self.t as f64;

        for (i, n) in nodes.iter_mut().enumerate() {
            if !n.online {
                n.cpu_pct = 0.0;
                n.ram_pct = 0.0;
                Self::push_hist(&mut n.cpu_history, 0.0);
                Self::push_hist(&mut n.ram_history, 0.0);
                continue;
            }
            let phase = t * 0.35 + i as f64 * 1.7;
            let drift = ((t * 0.03 + i as f64).sin()) * 2.5;
            n.cpu_pct = Self::organic(n.cpu_pct.max(8.0), phase, 6.0, drift);
            n.ram_pct = Self::organic(n.ram_pct.max(10.0), phase + 0.9, 3.5, drift * 0.4);
            Self::push_hist(&mut n.cpu_history, n.cpu_pct);
            Self::push_hist(&mut n.ram_history, n.ram_pct);
        }

        for (j, c) in containers.iter_mut().enumerate() {
            let phase = t * 0.4 + j as f64 * 0.9;
            c.cpu_share_pct = Self::organic(c.cpu_share_pct.max(3.0), phase, 4.5, 0.0).min(95.0);
            c.ram_share_pct =
                Self::organic(c.ram_share_pct.max(4.0), phase + 0.6, 2.8, 0.0).min(95.0);
            c.status = "running".into();
            if self.t.is_multiple_of(5) && c.logs.len() < 120 {
                c.logs.push(format!(
                    "[tick {}] {} healthy cpu={:.1}% ram={:.1}%",
                    self.t, c.name, c.cpu_share_pct, c.ram_share_pct
                ));
            }
        }
        // custos proporcionais são recalculados dinamicamente pela UI via allocate_costs a cada render
    }
}

fn mk_container(id: &str, name: &str, node_id: &str, stack: &str, cpu: f64, ram: f64) -> Container {
    Container {
        id: id.into(),
        name: name.into(),
        node_id: node_id.into(),
        stack: stack.into(),
        cpu_share_pct: cpu,
        ram_share_pct: ram,
        status: "running".into(),
        logs: Vec::new(),
    }
}

fn sample_logs(name: &str) -> Vec<String> {
    vec![
        format!("[init] {name} started"),
        format!("[health] {name} readiness ok"),
        format!("[metrics] {name} exporting telemetry"),
    ]
}

#[derive(Debug, Clone)]
pub struct DiscoveredContainer {
    id: String,
    name: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct StatsRow {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "CPUPerc", default)]
    cpu: String,
    #[serde(rename = "MemPerc", default)]
    memory: String,
    #[serde(rename = "Container", default)]
    container: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ParsedStats {
    pub cpu_pct: f64,
    pub mem_pct: f64,
}

fn run_docker(args: &[&str], timeout: Duration) -> Result<std::process::Output, String> {
    let mut child = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("falha ao executar docker: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut s) = child.stdout.take() {
                let _ = s.read_to_end(&mut stdout);
            }
            if let Some(mut s) = child.stderr.take() {
                let _ = s.read_to_end(&mut stderr);
            }
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "timeout ao executar docker {}",
                args.first().copied().unwrap_or("")
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn collect(
    products: &[Product],
    _history: &mut HashMap<String, (VecDeque<u64>, VecDeque<u64>)>,
) -> Result<Snapshot, String> {
    // Uma verificação leve: se docker ps falhar por daemon indisponível, tratamos como indisponível.
    let mut nodes = Vec::new();
    let mut containers = Vec::new();
    let mut had_any_success = false;
    let mut last_err: Option<String> = None;

    for product in products {
        let project = format!("kodoo-{}", product.label());
        match list_project_containers(&project) {
            Ok(discovered) => {
                had_any_success = true;
                let ids: Vec<&str> = discovered.iter().map(|c| c.id.as_str()).collect();
                let stats = read_stats(&ids);
                let node_id = format!("docker-{}", product.label());
                let mut node_cpu = 0.0;
                let mut node_ram = 0.0;
                for item in discovered {
                    let stat = stats.get(&item.id).or_else(|| stats.get(&item.name));
                    let cpu = stat.map(|s| s.cpu_pct).unwrap_or(0.0);
                    let ram = stat.map(|s| s.mem_pct).unwrap_or(0.0);
                    node_cpu += cpu;
                    node_ram += ram;
                    containers.push(Container {
                        id: item.id,
                        name: item.name,
                        node_id: node_id.clone(),
                        stack: product.label().to_string(),
                        cpu_share_pct: cpu,
                        ram_share_pct: ram,
                        status: item.status,
                        logs: Vec::new(),
                    });
                }
                nodes.push(Node {
                    id: node_id,
                    name: format!("docker-{}", product.label()),
                    region: "local".into(),
                    host: "local Docker context".into(),
                    ssh_user: "local".into(),
                    provider: "Docker Engine".into(),
                    monthly_cost: 0.0,
                    online: true,
                    cpu_pct: node_cpu.clamp(0.0, 100.0),
                    ram_pct: node_ram.clamp(0.0, 100.0),
                    cpu_history: vec![node_cpu.round().clamp(0.0, 100.0) as u64],
                    ram_history: vec![node_ram.round().clamp(0.0, 100.0) as u64],
                });
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    if !had_any_success {
        return Err(last_err.unwrap_or_else(|| "Docker indisponível".into()));
    }

    Ok(Snapshot {
        nodes,
        containers,
        meta: TelemetryMeta {
            last_success: Some(Instant::now()),
            last_error: last_err,
            docker_available: true,
        },
    })
}

pub fn list_project_containers(project: &str) -> Result<Vec<DiscoveredContainer>, String> {
    let filter = format!("label=com.docker.compose.project={project}");
    let output = run_docker(
        &[
            "ps",
            "--all",
            "--filter",
            &filter,
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.State}}\t{{.Label \"com.docker.compose.service\"}}",
        ],
        Duration::from_secs(4),
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "docker ps falhou".into()
        } else {
            stderr
        });
    }
    Ok(parse_ps_output(&String::from_utf8_lossy(&output.stdout)))
}

pub fn parse_ps_output(output: &str) -> Vec<DiscoveredContainer> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            let id = fields.next()?.trim();
            let name = fields.next()?.trim();
            let state = fields.next()?.trim();
            let service = fields.next().unwrap_or_default().trim();
            if id.is_empty() || name.is_empty() {
                return None;
            }
            Some(DiscoveredContainer {
                id: id.to_string(),
                name: if service.is_empty() {
                    name.to_string()
                } else {
                    format!("{service} ({name})")
                },
                status: state.to_string(),
            })
        })
        .collect()
}

pub fn read_stats(ids: &[&str]) -> HashMap<String, ParsedStats> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let mut args: Vec<&str> = vec!["stats", "--no-stream", "--format", "{{json .}}"];
    args.extend_from_slice(ids);
    let output = match run_docker(&args, Duration::from_secs(5)) {
        Ok(output) if output.status.success() => output,
        _ => return HashMap::new(),
    };
    parse_stats_output(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_stats_output(output: &str) -> HashMap<String, ParsedStats> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<StatsRow>(line).ok())
        .map(|row| {
            let key = if !row.id.is_empty() {
                row.id
            } else if !row.container.is_empty() {
                row.container
            } else {
                row.name.clone()
            };
            (
                key,
                ParsedStats {
                    cpu_pct: parse_percent(&row.cpu),
                    mem_pct: parse_percent(&row.memory),
                },
            )
        })
        .collect()
}

pub fn parse_percent(value: &str) -> f64 {
    value
        .trim()
        .trim_end_matches('%')
        .replace(',', ".")
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_percentages() {
        assert_eq!(parse_percent("12.50%"), 12.5);
        assert_eq!(parse_percent("0,75%"), 0.75);
        assert_eq!(parse_percent("--"), 0.0);
    }

    #[test]
    fn parses_ps_output_with_service() {
        let out = "abc123\tkodoo-demo-backend-1\trunning\tbackend\n";
        let items = parse_ps_output(out);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "abc123");
        assert!(items[0].name.contains("backend"));
        assert_eq!(items[0].status, "running");
    }

    #[test]
    fn parses_stats_output_tolerates_invalid_lines() {
        let out =
            "{\"ID\":\"abc\",\"Name\":\"n\",\"CPUPerc\":\"1.5%\",\"MemPerc\":\"2.0%\"}\n{bad}\n";
        let stats = parse_stats_output(out);
        assert_eq!(stats.len(), 1);
        assert!((stats.get("abc").unwrap().cpu_pct - 1.5).abs() < 0.001);
    }

    #[test]
    fn empty_snapshot_is_ok() {
        let items = parse_ps_output("");
        assert!(items.is_empty());
        let stats = parse_stats_output("");
        assert!(stats.is_empty());
    }
}
