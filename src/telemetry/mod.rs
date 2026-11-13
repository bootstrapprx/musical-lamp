use rand::Rng;

use crate::domain::{Container, Node};

pub trait TelemetryProvider {
    fn tick(&mut self, nodes: &mut [Node], containers: &mut [Container]);
}

pub struct MockTelemetryProvider {
    t: u64,
}

impl MockTelemetryProvider {
    pub fn new() -> Self {
        Self { t: 0 }
    }
}

impl TelemetryProvider for MockTelemetryProvider {
    fn tick(&mut self, nodes: &mut [Node], containers: &mut [Container]) {
        self.t = self.t.wrapping_add(1);
        let mut rng = rand::thread_rng();
        for node in nodes.iter_mut() {
            if !node.online {
                node.cpu_pct = 0.0;
                node.ram_pct = 0.0;
            } else {
                let drift = (self.t as f64 * 0.3 + node.name.len() as f64).sin() * 8.0;
                node.cpu_pct = (node.cpu_pct + drift * 0.1 + rng.gen_range(-3.0..3.0)).clamp(2.0, 96.0);
                node.ram_pct = (node.ram_pct + drift * 0.05 + rng.gen_range(-2.0..2.0)).clamp(10.0, 94.0);
            }
            node.cpu_history.push(node.cpu_pct.round() as u64);
            node.ram_history.push(node.ram_pct.round() as u64);
            if node.cpu_history.len() > 60 {
                node.cpu_history.remove(0);
            }
            if node.ram_history.len() > 60 {
                node.ram_history.remove(0);
            }
        }
        // distribute shares per node proportionally-ish
        for node in nodes.iter() {
            let idxs: Vec<usize> = containers
                .iter()
                .enumerate()
                .filter(|(_, c)| c.node_id == node.id)
                .map(|(i, _)| i)
                .collect();
            if idxs.is_empty() {
                continue;
            }
            let n = idxs.len() as f64;
            let base_cpu = if node.online { node.cpu_pct / n.max(1.0) } else { 0.0 };
            let base_ram = if node.online { node.ram_pct / n.max(1.0) } else { 0.0 };
            for (j, i) in idxs.iter().enumerate() {
                let wobble = ((self.t as f64 * 0.7) + j as f64).cos() * 4.0;
                containers[*i].cpu_share_pct = (base_cpu + wobble + rng.gen_range(-2.0..2.0)).clamp(0.0, 100.0);
                containers[*i].ram_share_pct = (base_ram + wobble * 0.5 + rng.gen_range(-1.5..1.5)).clamp(0.0, 100.0);
                if node.online && self.t.is_multiple_of(2) {
                    let msg = format!(
                        "[tick {}] {} cpu={:.1}% ram={:.1}% ok",
                        self.t, containers[*i].name, containers[*i].cpu_share_pct, containers[*i].ram_share_pct
                    );
                    containers[*i].logs.push(msg);
                    if containers[*i].logs.len() > 200 {
                        containers[*i].logs.remove(0);
                    }
                }
            }
        }
    }
}
