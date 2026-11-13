use slt::{Border, ButtonVariant, Color, Context, RunConfig, SelectState, TextInputState, Theme};
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let mut projectname = TextInputState::with_placeholder("Project name (my-cli-app)");
    let mut language = SelectState::new(vec!["TypeScript", "Rust", "Python", "Go"]);
    let mut terms = true;

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
                    ui.text("CLI Wizard").bold().fg(Color::Cyan);
                    ui.badge("step 2/3");
                    ui.spacer();
                    ui.text("Create your project scaffold").dim();
                });
                ui.bordered(Border::Single).pad(1).gap(1).grow(1).col(|ui| {
                    ui.text("Step 2: Project Settings").bold();
                    ui.separator();
                    let _ = ui.text_input(&mut projectname);
                    let _ = ui.select(&mut language);
                    let _ = ui.checkbox(
                        "I agree to create config files in the current directory",
                        &mut terms,
                    );
                    ui.container().gap(1).row(|ui| {
                        if ui.button_with("Back", ButtonVariant::Outline).clicked {
                            // TODO: Handle button click
                        }
                        if ui.button("Next").clicked {
                            // TODO: Handle button click
                        }
                    });
                });
                let _ = ui.help(&[
                    ("q", "quit"),
                    ("Tab", "focus"),
                    ("Enter", "next"),
                    ("Space", "toggle"),
                    ("?", "help"),
                ]);
            });
        },
    )
}
