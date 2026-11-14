//! Telemetria local do Docker.
//!
//! A TUI consulta o daemon por meio do CLI oficial em vez de assumir a presença
//! de uma API/socket específico. Isso mantém a primeira integração compatível
//! com Docker Desktop, Linux e contextos Docker remotos configurados no CLI.

use std::collections::HashMap;
use std::process::Command;

use serde::Deserialize;

use crate::domain::{Container, Node};

pub trait TelemetryProvider {
    fn tick(&mut self, nodes: &mut Vec<Node>, containers: &mut Vec<Container>);
}

pub struct DockerTelemetryProvider {
    products: Vec<&'static str>,
}

impl DockerTelemetryProvider {
    pub fn new() -> Self {
        Self {
            products: vec!["accounting", "trivia"],
        }
    }

    fn collect(&self) -> Option<(Vec<Node>, Vec<Container>)> {
        if !docker_available() {
            return None;
        }

        let mut nodes = Vec::new();
        let mut containers = Vec::new();
        for product in &self.products {
            let project = format!("kodoo-{product}");
            let discovered = list_project_containers(&project)?;
            let ids: Vec<&str> = discovered.iter().map(|c| c.id.as_str()).collect();
            let stats = read_stats(&ids);
            let node_id = format!("docker-{product}");

            let mut node_cpu = 0.0;
            let mut node_ram = 0.0;
            for item in discovered {
                let stat = stats.get(&item.id);
                let cpu = stat.map(|s| s.cpu_pct).unwrap_or(0.0);
                let ram = stat.map(|s| s.mem_pct).unwrap_or(0.0);
                node_cpu += cpu;
                node_ram += ram;
                containers.push(Container {
                    id: item.id,
                    name: item.name,
                    node_id: node_id.clone(),
                    stack: (*product).to_string(),
                    cpu_share_pct: cpu,
                    ram_share_pct: ram,
                    status: item.status,
                    logs: Vec::new(),
                });
            }

            nodes.push(Node {
                id: node_id,
                name: format!("docker-{product}"),
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
        Some((nodes, containers))
    }
}

impl Default for DockerTelemetryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryProvider for DockerTelemetryProvider {
    fn tick(&mut self, nodes: &mut Vec<Node>, containers: &mut Vec<Container>) {
        let Some((fresh_nodes, fresh_containers)) = self.collect() else {
            for node in nodes.iter_mut() {
                node.online = false;
                node.cpu_pct = 0.0;
                node.ram_pct = 0.0;
            }
            for container in containers.iter_mut() {
                container.status = "docker indisponível".into();
                container.cpu_share_pct = 0.0;
                container.ram_share_pct = 0.0;
            }
            return;
        };

        *nodes = fresh_nodes;
        *containers = fresh_containers;
    }
}

#[cfg(test)]
pub struct MockTelemetryProvider;

#[cfg(test)]
impl MockTelemetryProvider {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
impl TelemetryProvider for MockTelemetryProvider {
    fn tick(&mut self, _nodes: &mut Vec<Node>, _containers: &mut Vec<Container>) {}
}

#[derive(Debug, Clone)]
struct DiscoveredContainer {
    id: String,
    name: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct StatsRow {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "CPUPerc")]
    cpu: String,
    #[serde(rename = "MemPerc")]
    memory: String,
}

#[derive(Debug, Clone, Copy)]
struct ParsedStats {
    cpu_pct: f64,
    mem_pct: f64,
}

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn list_project_containers(project: &str) -> Option<Vec<DiscoveredContainer>> {
    let filter = format!("label=com.docker.compose.project={project}");
    let output = Command::new("docker")
        .args([
            "ps",
            "--all",
            "--filter",
            &filter,
            "--format",
            "{{.ID}}\t{{.Names}}\t{{.State}}\t{{.Label \"com.docker.compose.service\"}}",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.lines()
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
            .collect(),
    )
}

fn read_stats(ids: &[&str]) -> HashMap<String, ParsedStats> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let output = match Command::new("docker")
        .arg("stats")
        .arg("--no-stream")
        .arg("--format")
        .arg("{{json .}}")
        .args(ids)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return HashMap::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<StatsRow>(line).ok())
        .map(|row| {
            let key = if row.id.is_empty() { row.name } else { row.id };
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

fn parse_percent(value: &str) -> f64 {
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
    use super::parse_percent;

    #[test]
    fn parses_docker_percentages() {
        assert_eq!(parse_percent("12.50%"), 12.5);
        assert_eq!(parse_percent("0,75%"), 0.75);
        assert_eq!(parse_percent("--"), 0.0);
    }
}
