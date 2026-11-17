mod action;
mod app;
mod blueprint;
mod control_plane;
mod domain;
mod event;
mod executor;
mod ops_core;
mod orchestration;
mod planner;
mod registry;
mod state;
mod telemetry;
mod ui;

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::app::AppState;
use crate::domain::discover_products;
use crate::event::{Action, Event};
use crate::telemetry::{MockTelemetryProvider, TelemetryProvider};

/// Raiz = diretorio que contem common/compose.base.yaml. Ordem: argumento, MUSICAL_LAMP_ROOT,
/// subindo a partir do cwd e, por fim, a partir do executavel (target/debug/...).
fn find_root() -> Option<PathBuf> {
    let has_marker = |d: &Path| d.join("common").join("compose.base.yaml").is_file();
    let walk_up = |start: PathBuf| -> Option<PathBuf> {
        let mut cur = Some(start.as_path());
        while let Some(d) = cur {
            if has_marker(d) {
                return Some(d.to_path_buf());
            }
            cur = d.parent();
        }
        None
    };
    if let Some(arg) = std::env::args().nth(1).filter(|a| !a.starts_with('-')) {
        let p = PathBuf::from(arg);
        return has_marker(&p).then_some(p);
    }
    if let Ok(r) = std::env::var("MUSICAL_LAMP_ROOT") {
        let p = PathBuf::from(r);
        return has_marker(&p).then_some(p);
    }
    std::env::current_dir().ok().and_then(walk_up).or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.to_path_buf()))
            .and_then(walk_up)
    })
}

/// Restaura o terminal se houver panic (senao o shell fica em modo raw/tela alternativa).
fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        prev(info);
    }));
}

#[tokio::main]
async fn main() -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!("frappe-deployer precisa de um terminal interativo (TTY); stdin/stdout nao sao um terminal.");
        std::process::exit(2);
    }
    match find_root() {
        Some(root) => std::env::set_current_dir(root)?,
        None => {
            eprintln!(
                "nao encontrei common/compose.base.yaml. Rode a partir do repositorio, \
                 passe o diretorio como argumento ou defina MUSICAL_LAMP_ROOT."
            );
            std::process::exit(2);
        }
    }
    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
    spawn_input_reader(tx.clone());
    spawn_tick(tx.clone());

    let _products = discover_products();
    // MockTelemetryProvider: demonstração imediata com pool multi-servidor, flutuação orgânica e custos dinâmicos.
    let provider: Box<dyn TelemetryProvider + Send> = Box::new(MockTelemetryProvider::new());
    let mut state = AppState::new(provider);

    let res = run_app(&mut terminal, &mut state, &mut rx, tx.clone()).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    rx: &mut mpsc::UnboundedReceiver<Event>,
    tx: mpsc::UnboundedSender<Event>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, state))?;
        if state.should_quit {
            return Ok(());
        }
        if let Some(event) = rx.recv().await {
            match state.handle_event(event) {
                Some(Action::Quit) => state.should_quit = true,
                Some(action @ Action::RunOps { .. }) => {
                    orchestration::spawn_ops_command(tx.clone(), action);
                }
                None => {}
            }
        }
    }
}

fn spawn_input_reader(tx: mpsc::UnboundedSender<Event>) {
    tokio::task::spawn_blocking(move || loop {
        if crossterm::event::poll(Duration::from_millis(100)).unwrap_or(false) {
            match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(key))
                    if matches!(
                        key.kind,
                        crossterm::event::KeyEventKind::Press
                            | crossterm::event::KeyEventKind::Repeat
                    ) =>
                {
                    if tx.send(Event::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(crossterm::event::Event::Resize(_, _)) => {
                    if tx.send(Event::Resize).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
}

fn spawn_tick(tx: mpsc::UnboundedSender<Event>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if tx.send(Event::Tick).is_err() {
                break;
            }
        }
    });
}
