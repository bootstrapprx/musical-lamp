/// Docker orchestration module
/// Connects TUI actions to real docker compose operations via common/dc.sh
/// Provides live status updates and error handling

use std::process::{Child, Stdio};
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::time::sleep;

use crate::domain::{Product, StackState, OpsStatus, OpsStatusUpdate};

#[derive(Debug, Clone, Copy)]
pub enum DockerCmd {
    Up,
    Down,
    Logs,
    Migrate,
    Status,
}

pub struct DockerOrchestrator {
    tx: Sender<OpsStatusUpdate>,
    product: Product,
    process: Option<Child>,
    last_log_check: Instant,
    log_buffer: Vec<String>,
}

impl DockerOrchestrator {
    pub fn new(tx: Sender<OpsStatusUpdate>, product: Product) -> Self {
        Self {
            tx,
            product,
            process: None,
            last_log_check: Instant::now(),
            log_buffer: Vec::new(),
        }
        }
        }
        }

    pub async fn run(&mut self) {
        loop {
            if let Some(child) = &mut self.process {
                if child.try_wait().is_ok() {
                    // Process exited
                    let status = self.process.take();
                    self.tx.send(OpsStatusUpdate {
                        product: self.product,
                        state: StackState::Stopped,
                        message: "Docker process exited".to_string(),
                    }).await.unwrap_or(());
                    break;
                }
            }

            self.check_logs().await;
            sleep(Duration::from_millis(500)).await;
        }
    }

    async fn check_logs(&mut self) {
        if let Some(child) = &self.process {
            let since = self.last_log_check;
            self.last_log_check = Instant::now();
            if let Some(stdout) = child.stdout.as_mut() {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Some(line) = lines.next().await? {
                    let line = line.unwrap_or_default();
                    if line.contains("error") || line.contains("ERROR") || line.contains("Failed") {
                        self.tx.send(OpsStatusUpdate {
                            product: self.product,
                            state: StackState::Degraded,
                            message: format!("Docker error: {}", line),
                        }).await.unwrap_or(());
                    }
                    self.log_buffer.push(line);
                    if self.log_buffer.len() > 50 {
                        self.log_buffer.drain(0..self.log_buffer.len()-50);
                    }
                }
            }
        }
    }

    pub async fn execute(&mut self, cmd: DockerCmd) {
        if self.process.is_some() {
            // Kill existing process
            if let Some(mut child) = self.process.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        let product_label = self.product.label();
        let mut command = std::process::Command::new("bash");
        command.arg("common/dc.sh");
        command.arg(product_label);
        command.arg(match cmd {
            DockerCmd::Up => "up",
            DockerCmd::Down => "down",
            DockerCmd::Logs => "logs",
            DockerCmd::Migrate => "run --rm ops bench --site SITE migrate",
            DockerCmd::Status => "ps",
        });
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        match command.spawn() {
            Ok(child) => {
                self.process = Some(child);
                self.tx.send(OpsStatusUpdate {
                    product: self.product,
                    state: StackState::Starting,
                    message: format!("Started Docker cmd: {:?}", cmd),
                }).await.unwrap_or(());
            }
            Err(e) => {
                self.tx.send(OpsStatusUpdate {
                    product: self.product,
                    state: StackState::Degraded,
                    message: format!("Failed to start Docker: {}", e),
                }).await.unwrap_or(());
            }
        }
    }
}

/// Orchestration manager for all products
pub struct OrchestrationManager {
    orchestrators: HashMap<Product, DockerOrchestrator>,
    tx: Sender<OpsStatusUpdate>,
}

impl OrchestrationManager {
    pub fn new(tx: Sender<OpsStatusUpdate>) -> Self {
        let mut orchestrators = HashMap::new();
        for product in [Product::Accounting, Product::Trivia] {
            orchestrators.insert(product, DockerOrchestrator::new(tx.clone(), product));
        }
        Self { orchestrators, tx }
    }

    pub async fn start_monitoring(&mut self) {
        for orch in self.orchestrators.values_mut() {
            tokio::spawn(async move {
                orch.run().await;
            });
        }
    }

    pub async fn execute(&self, product: Product, cmd: DockerCmd) {
        if let Some(orch) = self.orchestrators.get(&product) {
            orch.execute(cmd).await;
        }
    }
}

/// Message type for orchestrator updates
#[derive(Debug, Clone)]
pub struct OpsStatusUpdate {
    pub product: Product,
    pub state: StackState,
    pub message: String,
}
