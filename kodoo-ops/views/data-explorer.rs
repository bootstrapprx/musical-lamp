use slt::{Border, Color, Context, ListState, RunConfig, TableState, TextInputState, Theme};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut datasets = ListState::new(vec![
        "api_requests",
        "auth_events",
        "billing_tx",
        "checkout_sessions",
        "deploy_logs",
        "error_events",
        "latency_samples",
        "region_metrics",
    ]);
    let mut search = TextInputState::with_placeholder("Search by service, region, owner...");
    let mut results = TableState::new(
        vec!["Service", "Region", "Owner", "Latency"],
        vec![
            vec!["api-gateway", "eu-west-1", "platform", "22 ms"],
            vec!["auth-service", "eu-west-1", "identity", "17 ms"],
            vec!["checkout-api", "us-east-1", "commerce", "142 ms"],
            vec!["billing-worker", "us-east-1", "finance", "91 ms"],
            vec!["search-indexer", "ap-northeast-2", "search", "211 ms"],
            vec!["notification-bus", "eu-central-1", "platform", "44 ms"],
        ],
    );

    slt::run_with(
        RunConfig::default()
            .theme(Theme::dark())
            .mouse(true)
            .tick_rate(Duration::from_millis(16)),
        |ui: &mut Context| {
            if ui.key('q') {
                ui.quit();
            }

            ui.container().gap(1).grow(1).col(|ui| {
                ui.bordered(Border::Rounded).px(1).gap(1).row(|ui| {
                    ui.text("Data Explorer").bold().fg(Color::Cyan);
                    ui.badge("128 rows");
                    ui.spacer();
                    ui.text("warehouse: prod").dim();
                });
                ui.container().gap(1).grow(1).row(|ui| {
                    ui.bordered(Border::Single).w(25).pad(1).gap(1).col(|ui| {
                        ui.divider_text("Datasets");
                        let _ = ui.list(&mut datasets);
                    });
                    ui.bordered(Border::Single).pad(1).gap(1).grow(1).col(|ui| {
                        let _ = ui.text_input(&mut search);
                        let _ = ui.table(&mut results);
                        ui.separator();
                        ui.container().gap(1).row(|ui| {
                            ui.sparkline(
                                &[
                                    84.0, 92.0, 105.0, 99.0, 111.0, 124.0, 117.0, 131.0, 140.0,
                                    133.0, 146.0, 152.0,
                                ],
                                24,
                            );
                            ui.bar_chart(
                                &[
                                    ("KR", 12.0),
                                    ("JP", 9.0),
                                    ("US", 21.0),
                                    ("DE", 6.0),
                                    ("SG", 8.0),
                                ],
                                6,
                            );
                        });
                    });
                });
                let _ = ui.help(&[
                    ("q", "quit"),
                    ("Tab", "focus"),
                    ("Enter", "select"),
                    ("/", "search"),
                    ("?", "help"),
                ]);
            });
        },
    )
}
