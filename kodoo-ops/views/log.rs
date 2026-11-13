use slt::{Border, Color, Context, ListState, RunConfig, TextInputState, Theme};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut filter = TextInputState::with_placeholder("Filter logs (error, warn, auth...)");
    let mut entries = ListState::new(vec![
        "10:42:11 INFO api  request completed in 24ms",
        "10:42:12 WARN auth token refresh nearing expiry",
        "10:42:13 INFO jobs queued email digest job",
        "10:42:14 ERROR db   connection timeout retry=1",
        "10:42:15 INFO api  GET /v1/projects 200",
        "10:42:16 WARN cache miss ratio above threshold",
        "10:42:17 INFO ws   client connected id=92f1",
    ]);

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
                    ui.text("Log Viewer").bold().fg(Color::Cyan);
                    ui.badge("LIVE");
                    ui.spacer();
                    ui.text("tail -f app.log").dim();
                });
                ui.bordered(Border::Single).pad(1).gap(1).grow(1).col(|ui| {
                    let _ = ui.text_input(&mut filter);
                    let _ = ui.list(&mut entries);
                    ui.separator();
                    ui.container().gap(1).row(|ui| {
                        ui.stat("Total", "12,481");
                        ui.stat("Errors", "14");
                        ui.stat("Warnings", "63");
                    });
                    ui.container().gap(1).row(|ui| {
                        ui.badge("WARN");
                        ui.text("Latest issue: cache miss ratio reached 12%").dim();
                    });
                });
                let _ = ui.help(&[
                    ("q", "quit"),
                    ("Tab", "focus"),
                    ("Enter", "inspect"),
                    ("/", "filter"),
                    ("?", "help"),
                ]);
            });
        },
    )
}
