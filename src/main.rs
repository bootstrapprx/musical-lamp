mod action;
mod app;
mod domain;
mod telemetry;
mod ui;

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::action::{Action, Event};
use crate::app::AppState;
use crate::domain::Product;
use crate::telemetry::{MockTelemetryProvider, TelemetryProvider};

#[tokio::main]
async fn main() -> io::Result<()> {
    prompt_for_tunnel_token_if_missing(Product::Accounting)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
    spawn_input_reader(tx.clone());
    spawn_tick(tx.clone());

    let provider: Box<dyn TelemetryProvider + Send> = Box::new(MockTelemetryProvider::new());
    let mut state = AppState::new(provider);

    let res = run_app(&mut terminal, &mut state, &mut rx).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn prompt_for_tunnel_token_if_missing(product: Product) -> io::Result<()> {
    if app::env_has_tunnel_token(product) {
        return Ok(());
    }
    let label = product.label();
    println!("Cloudflare Tunnel: o container cloudflared sobe via Docker Compose, mas o token não está configurado para o produto \"{label}\".");
    println!("Informe o TUNNEL_TOKEN (deixe em branco para pular e configurar depois na aba .env):");
    print!("TUNNEL_TOKEN = ");
    io::stdout().flush()?;
    let mut token = String::new();
    io::stdin().read_line(&mut token)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        println!("Token não informado. Você poderá configurar em products/{label}/.env ou na aba [4] .env antes de subir o stack com cloudflared.");
        return Ok(());
    }
    app::set_tunnel_token(product, &token)?;
    println!("TUNNEL_TOKEN salvo em products/{label}/.env.");
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    rx: &mut mpsc::UnboundedReceiver<Event>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, state))?;
        if state.should_quit {
            return Ok(());
        }
        if let Some(event) = rx.recv().await {
            if let Some(Action::Quit) = state.handle_event(event) {
                state.should_quit = true;
            }
        }
    }
}

fn spawn_input_reader(tx: mpsc::UnboundedSender<Event>) {
    tokio::task::spawn_blocking(move || loop {
        if crossterm::event::poll(Duration::from_millis(100)).unwrap_or(false) {
            match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(key)) => {
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
