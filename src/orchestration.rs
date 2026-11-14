use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::domain::Product;
use crate::event::{Action, Event};

/// Build a real docker-compose orchestration action for the selected product.
/// Uses common/dc.sh <product> ... which wraps `docker compose` with the right env/base files.
pub fn build_ops_action(product: Product, action: &str) -> Option<Action> {
    let p = product.label().to_string();
    let (cmd, args): (String, Vec<String>) = match action {
        "up" => (
            "bash".into(),
            vec!["common/dc.sh".into(), p, "up".into(), "-d".into()],
        ),
        "down" => ("bash".into(), vec!["common/dc.sh".into(), p, "down".into()]),
        "status" => ("bash".into(), vec!["common/dc.sh".into(), p, "ps".into()]),
        "logs" => (
            "bash".into(),
            vec![
                "common/dc.sh".into(),
                p,
                "logs".into(),
                "--tail=80".into(),
                "--no-color".into(),
            ],
        ),
        "migrate" => (
            "bash".into(),
            vec![
                "common/dc.sh".into(),
                p,
                "run".into(),
                "--rm".into(),
                "ops".into(),
                "bench".into(),
                "--site".into(),
                "all".into(),
                "migrate".into(),
            ],
        ),
        _ => return None,
    };
    Some(Action::RunOps {
        product,
        action: action.to_string(),
        cmd,
        args,
    })
}

pub fn spawn_ops_command(tx: mpsc::UnboundedSender<Event>, req: Action) {
    let Action::RunOps {
        product,
        action,
        cmd,
        args,
    } = req
    else {
        return;
    };

    tokio::spawn(async move {
        let pretty = format!("{} {}", cmd, args.join(" "));
        let _ = tx.send(Event::OpsLine {
            product,
            line: format!("[exec] {pretty}"),
        });

        let mut child = match Command::new(&cmd)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Event::OpsFinished {
                    product,
                    action,
                    success: false,
                    summary: format!("não foi possível executar: {e}"),
                });
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx2.send(Event::OpsLine { product, line });
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx2.send(Event::OpsLine {
                        product,
                        line: format!("[err] {line}"),
                    });
                }
            });
        }

        match child.wait().await {
            Ok(status) => {
                let ok = status.success();
                let code = status.code();
                let summary = if ok {
                    format!("concluído (código {})", code.unwrap_or(0))
                } else {
                    format!("falhou (código {:?})", code)
                };
                let _ = tx.send(Event::OpsFinished {
                    product,
                    action,
                    success: ok,
                    summary,
                });
            }
            Err(e) => {
                let _ = tx.send(Event::OpsFinished {
                    product,
                    action,
                    success: false,
                    summary: format!("erro ao aguardar processo: {e}"),
                });
            }
        }
    });
}
