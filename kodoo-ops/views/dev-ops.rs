use slt::{AlertLevel, Border, Color, Context, RunConfig, TableState, Theme, Trend};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut services = TableState::new(
        vec!["Service", "Version", "Region", "Status"],
        vec![
            vec!["api-gateway", "2026.03.17.2", "eu-west-1", "healthy"],
            vec!["auth-service", "2026.03.17.1", "eu-west-1", "healthy"],
            vec!["billing-worker", "2026.03.16.8", "us-east-1", "degraded"],
            vec![
                "search-indexer",
                "2026.03.16.5",
                "ap-northeast-2",
                "healthy",
            ],
            vec![
                "notification-bus",
                "2026.03.15.9",
                "eu-central-1",
                "healthy",
            ],
            vec!["cdn-sync", "2026.03.14.4", "global", "maintenance"],
            vec!["metrics-ingest", "2026.03.16.2", "us-west-2", "healthy"],
            vec!["image-proxy", "2026.03.15.7", "ap-southeast-1", "healthy"],
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
                ui.bordered(Border::Rounded).px(2).gap(1).row(|ui| {
                    ui.text("Infrastructure").bold().fg(Color::Cyan);
                    ui.badge("3 alerts");
                    ui.spacer();
                    ui.text("cluster: prod-eu-1").dim();
                });
                ui.container().gap(1).row(|ui| {
                    ui.stat_trend("Req / min", "184k", Trend::Up);
                    ui.stat_trend("Error Rate", "0.21%", Trend::Down);
                    ui.stat_trend("P95 Latency", "128 ms", Trend::Up);
                    ui.stat_trend("Deploys / day", "42", Trend::Up);
                });
                let _ = ui.table(&mut services);
                let _ = ui.alert(
                    "billing-worker queue lag above 90 seconds",
                    AlertLevel::Error,
                );
                let _ = ui.alert(
                    "checkout-api cache miss rate above threshold",
                    AlertLevel::Warning,
                );
                let _ = ui.alert("deploy freeze starts at 17:00 UTC", AlertLevel::Info);
                let _ = ui.help(&[
                    ("q", "quit"),
                    ("Tab", "focus"),
                    ("Enter", "select"),
                    ("r", "refresh"),
                    ("?", "help"),
                ]);
            });
        },
    )
}
