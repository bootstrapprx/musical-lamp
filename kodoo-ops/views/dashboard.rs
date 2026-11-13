use slt::{Border, Color, Context, ListState, RunConfig, TableState, Theme};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut nav = ListState::new(vec![
        "Overview",
        "Services",
        "Deployments",
        "Incidents",
        "SLO Reports",
        "Capacity",
        "Billing",
        "Audit Trail",
    ]);
    let mut services = TableState::new(
        vec!["Service", "Status", "Uptime", "Latency"],
        vec![
            vec!["api-gateway", "healthy", "99.98%", "12 ms"],
            vec!["auth-service", "healthy", "99.96%", "17 ms"],
            vec!["billing-worker", "degraded", "98.71%", "142 ms"],
            vec!["search-indexer", "healthy", "99.61%", "31 ms"],
            vec!["notification-bus", "healthy", "99.93%", "22 ms"],
            vec!["cdn-sync", "maintenance", "99.40%", "18 ms"],
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
                    ui.text("System Dashboard").bold().fg(Color::Cyan);
                    ui.badge("Live");
                    ui.spacer();
                    ui.text("updated 09:41 UTC").dim();
                });
                ui.container().gap(1).grow(1).row(|ui| {
                    ui.bordered(Border::Single).w(25).pad(1).gap(1).col(|ui| {
                        ui.divider_text("Navigation");
                        let _ = ui.list(&mut nav);
                        ui.divider_text("Quick Facts");
                        ui.definition_list(&[
                            ("Cluster", "prod-eu-1"),
                            ("Nodes", "38 hosts"),
                            ("On-call", "platform-sre"),
                            ("Incidents", "1 open"),
                        ]);
                    });
                    ui.container().gap(1).grow(1).col(|ui| {
                        ui.container().gap(1).row(|ui| {
                            ui.stat("CPU Usage", "73%");
                            ui.stat("Memory", "8.2 / 16 GB");
                            ui.stat("Latency", "12 ms");
                            ui.stat("Requests", "19.4k / min");
                        });
                        let _ = ui.table(&mut services);
                        ui.bar_chart(
                            &[
                                ("Mon", 42.0),
                                ("Tue", 48.0),
                                ("Wed", 55.0),
                                ("Thu", 51.0),
                                ("Fri", 58.0),
                                ("Sat", 39.0),
                                ("Sun", 44.0),
                            ],
                            6,
                        );
                    });
                });
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
