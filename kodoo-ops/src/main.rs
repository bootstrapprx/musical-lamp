//! CLI headless ou TUI interativa sobre o mesmo nucleo com Ratatui.
use kodoo_ops::{
    find_root, secrets, AppState, Event, Job, Ops, Product, Result, Secret, SecretKind, Severity, Stream,
};
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, TableState, Tabs},
    Terminal,
};

const USAGE: &str = "\
uso: kodoo-ops [--root DIR] [comando]
  (sem argumentos inicia a TUI interativa Ratatui)
  secret hex|alnum|urlsafe <n>
  env init|show|check <produto>
  env get <produto> CHAVE
  env set <produto> CHAVE VALOR
  env gen <produto> CHAVE [hex|alnum|urlsafe N]
  build <produto> <release> [--push]
  compose <produto> <args...>              (ex.: compose trivia up -d)
  site list|new|install|migrate|backup <produto> [site] [apps...]
  release <produto> --image REF <site...>
produtos: trivia | accounting";

fn product(s: Option<&String>) -> Result<Product> {
    s.and_then(|s| Product::from_slug(s))
        .ok_or_else(|| kodoo_ops::Error::Invalid("produto invalido (trivia|accounting)".into()))
}

fn arg<'a>(a: &'a [String], i: usize, what: &str) -> Result<&'a String> {
    a.get(i).ok_or_else(|| kodoo_ops::Error::Invalid(format!("falta {what}")))
}

fn kind(a: &[String]) -> Result<SecretKind> {
    let n: usize = arg(a, 1, "tamanho")?.parse().map_err(|_| kodoo_ops::Error::Invalid("tamanho invalido".into()))?;
    match arg(a, 0, "tipo")?.as_str() {
        "hex" => Ok(SecretKind::Hex(n)),
        "alnum" => Ok(SecretKind::Alnum(n)),
        "urlsafe" => Ok(SecretKind::UrlSafe(n)),
        o => Err(kodoo_ops::Error::Invalid(format!("tipo desconhecido: {o}"))),
    }
}

/// Imprime os eventos ate o job terminar; devolve o sucesso.
fn follow(ops: &Ops, id: kodoo_ops::JobId) -> bool {
    loop {
        for ev in ops.poll() {
            match ev {
                Event::Started { title, .. } => eprintln!("== {title}"),
                Event::StepStarted { index, total, title, .. } => eprintln!("-- [{}/{total}] {title}", index + 1),
                Event::Line { stream: Stream::Out, text, .. } => println!("{text}"),
                Event::Line { stream: Stream::Err, text, .. } => eprintln!("{text}"),
                Event::Finished { id: fid, ok, code } if fid == id => {
                    eprintln!("== {} (codigo {:?})", if ok { "ok" } else { "FALHOU" }, code);
                    return ok;
                }
                Event::Finished { .. } => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

struct TuiState {
    current_tab: usize, // 0: Dashboard, 1: DevOps, 2: System Monitor, 3: Data Explorer, 4: Logs, 5: Wizard
    env_table_state: TableState,
    services_table_state: TableState,
    input_buffer: String,
    editing_key: Option<String>,
    wizard_step: usize,
    wizard_site_input: String,
    wizard_apps_input: String,
    wizard_image_input: String,
}

impl TuiState {
    fn new() -> Self {
        let mut env_table = TableState::default();
        env_table.select(Some(0));
        let mut services_table = TableState::default();
        services_table.select(Some(0));
        Self {
            current_tab: 0,
            env_table_state: env_table,
            services_table_state: services_table,
            input_buffer: String::new(),
            editing_key: None,
            wizard_step: 0,
            wizard_site_input: "mysite.localhost".into(),
            wizard_apps_input: "payments,two_factor".into(),
            wizard_image_input: "ghcr.io/kodoo/prod:latest".into(),
        }
    }
}

fn run() -> Result<bool> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut root: Option<PathBuf> = None;
    if args.first().map(String::as_str) == Some("--root") {
        args.remove(0);
        root = Some(PathBuf::from(arg(&args, 0, "DIR")?));
        args.remove(0);
    }
    let root = match root {
        Some(r) => r,
        None => find_root(&std::env::current_dir()?)
            .ok_or_else(|| kodoo_ops::Error::NotFound("deployment/ (use --root)".into()))?,
    };

    if args.is_empty() {
        // Launch interactive Ratatui TUI
        enable_raw_mode().map_err(|e| kodoo_ops::Error::Io(e))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(|e| kodoo_ops::Error::Io(e))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|e| kodoo_ops::Error::Io(e))?;

        let mut app_state = AppState::new(root)?;
        let mut tui_state = TuiState::new();

        let res = run_tui(&mut terminal, &mut app_state, &mut tui_state);

        disable_raw_mode().map_err(|e| kodoo_ops::Error::Io(e))?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| kodoo_ops::Error::Io(e))?;
        terminal.show_cursor().map_err(|e| kodoo_ops::Error::Io(e))?;

        if let Err(err) = res {
            eprintln!("Erro na TUI: {err:?}");
            return Err(err);
        }

        return Ok(true);
    }

    if args[0] == "secret" {
        println!("{}", secrets::generate(kind(&args[1..])?)?);
        return Ok(true);
    }

    let ops = Ops::open(root)?;
    match args[0].as_str() {
        "env" => {
            let sub = arg(&args, 1, "subcomando")?.as_str();
            let p = product(args.get(2))?;
            match sub {
                "init" => {
                    ops.env_init(p)?;
                    println!("criado {} (DB_PASSWORD gerado)", ops.paths(p).env().display());
                }
                "show" => {
                    let e = ops.env_load(p)?;
                    for k in e.keys() {
                        let v = e.get(k).unwrap_or("");
                        println!("{k}={}", if secrets::is_secret_key(k) { secrets::mask(v) } else { v.to_string() });
                    }
                }
                "get" => println!("{}", ops.env_load(p)?.get(arg(&args, 3, "CHAVE")?).unwrap_or("")),
                "set" => {
                    let mut e = ops.env_load(p)?;
                    let k = arg(&args, 3, "CHAVE")?;
                    if let Some(n) = secrets::rotation_note(k) {
                        eprintln!("ATENCAO: {n}");
                    }
                    e.set(k, arg(&args, 4, "VALOR")?)?;
                    ops.env_save(p, &e)?;
                }
                "gen" => {
                    let k = arg(&args, 3, "CHAVE")?;
                    let kd = if args.len() > 4 {
                        kind(&args[4..])?
                    } else {
                        secrets::default_kind_for(k).unwrap_or(SecretKind::Hex(24))
                    };
                    let mut e = ops.env_load(p)?;
                    let v = secrets::generate(kd)?;
                    if let Some(n) = secrets::rotation_note(k) {
                        eprintln!("ATENCAO: {n}");
                    }
                    e.set(k, &v)?;
                    ops.env_save(p, &e)?;
                    println!("{k}={v}");
                }
                "check" => {
                    let findings = ops.validate(p);
                    for f in &findings {
                        println!("[{:?}] {}: {}", f.severity, f.key.as_deref().unwrap_or("-"), f.message);
                    }
                    if findings.iter().any(|f| f.severity == Severity::Error) {
                        return Ok(false);
                    }
                }
                o => return Err(kodoo_ops::Error::Invalid(format!("subcomando env desconhecido: {o}"))),
            }
        }
        "build" => {
            let p = product(args.get(1))?;
            let release = arg(&args, 2, "versao/release")?;
            let push = args.iter().any(|s| s == "--push");
            let id = ops.run(p, Job::Build { release: release.clone(), push });
            return Ok(follow(&ops, id));
        }
        "compose" => {
            let p = product(args.get(1))?;
            let ca: Vec<String> = args[2..].iter().map(String::to_string).collect();
            if ca.is_empty() {
                return Err(kodoo_ops::Error::Invalid("falta argumentos para compose".into()));
            }
            let id = ops.run(p, Job::Compose { args: ca });
            return Ok(follow(&ops, id));
        }
        "site" => {
            let sub = arg(&args, 1, "subcomando")?.as_str();
            let p = product(args.get(2))?;
            match sub {
                "list" => {
                    let id = ops.run(p, Job::ListSites);
                    return Ok(follow(&ops, id));
                }
                "new" => {
                    let site = arg(&args, 3, "site")?;
                    let pw = secrets::alnum(24)?;
                    println!("Gerado Administrator password: {pw}");
                    let id = ops.run(p, Job::NewSite { site: site.clone(), admin_password: Secret::new(pw) });
                    return Ok(follow(&ops, id));
                }
                "install" => {
                    let site = arg(&args, 3, "site")?;
                    let apps: Vec<String> = args[4..].iter().map(String::to_string).collect();
                    let id = ops.run(p, Job::InstallApps { site: site.clone(), apps });
                    return Ok(follow(&ops, id));
                }
                "migrate" => {
                    let site = arg(&args, 3, "site")?;
                    let id = ops.run(p, Job::Migrate { site: site.clone() });
                    return Ok(follow(&ops, id));
                }
                "backup" => {
                    let site = arg(&args, 3, "site")?;
                    let id = ops.run(p, Job::Backup { site: site.clone() });
                    return Ok(follow(&ops, id));
                }
                o => return Err(kodoo_ops::Error::Invalid(format!("subcomando site desconhecido: {o}"))),
            }
        }
        "release" => {
            let p = product(args.get(1))?;
            let mut image_ref: Option<String> = None;
            let mut sites = Vec::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--image" {
                    image_ref = Some(arg(&args, i + 1, "REF")?.clone());
                    i += 2;
                } else {
                    sites.push(args[i].clone());
                    i += 1;
                }
            }
            let image_ref = image_ref.ok_or_else(|| kodoo_ops::Error::Invalid("falta --image REF".into()))?;
            if sites.is_empty() {
                return Err(kodoo_ops::Error::Invalid("informe ao menos um site para release".into()));
            }
            let id = ops.release(p, &sites, &image_ref);
            return Ok(follow(&ops, id));
        }
        o => return Err(kodoo_ops::Error::Invalid(format!("comando desconhecido: {o}"))),
    }
    Ok(true)
}

fn run_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut AppState,
    tui: &mut TuiState,
) -> Result<()> {
    let titles = vec!["Dashboard", "DevOps", "System Monitor", "Data Explorer", "Logs", "Wizard"];

    loop {
        app.tick();

        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header & Tabs
                    Constraint::Min(10),   // Main content
                    Constraint::Length(3), // Footer / Status bar
                ])
                .split(size);

            // --- Header & Product Selector ---
            let tabs = Tabs::new(titles.iter().copied().map(Line::from).collect::<Vec<_>>())
                .block(Block::default().borders(Borders::ALL).title(format!(
                    " Kodoo Ops [{}] (Product: {:?}) ",
                    if app.busy() { "BUSY" } else { "READY" },
                    app.product
                )))
                .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                .select(tui.current_tab);
            f.render_widget(tabs, chunks[0]);

            // --- Main Content Area ---
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
                .split(chunks[1]);

            match tui.current_tab {
                0 => render_dashboard(f, main_chunks[0], main_chunks[1], app),
                1 => render_devops(f, main_chunks[0], main_chunks[1], app),
                2 => render_monitor(f, main_chunks[0], main_chunks[1], app),
                3 => render_explorer(f, main_chunks[0], main_chunks[1], app, tui),
                4 => render_logs(f, main_chunks[0], main_chunks[1], app),
                5 => render_wizard(f, main_chunks[0], main_chunks[1], app, tui),
                _ => {}
            }

            // --- Footer ---
            let msg = app.message.as_deref().unwrap_or("Press Tab/Shift+Tab or 1-6 to switch tabs. 'q' to quit. 'r' refresh status, 's' refresh sites.");
            let footer = Paragraph::new(msg)
                .block(Block::default().borders(Borders::ALL).title("Status / Messages"))
                .style(Style::default().fg(Color::Cyan));
            f.render_widget(footer, chunks[2]);
        }).map_err(|e| kodoo_ops::Error::Io(e))?;

        if event::poll(Duration::from_millis(30)).map_err(|e| kodoo_ops::Error::Io(e))? {
            if let CEvent::Key(key) = event::read().map_err(|e| kodoo_ops::Error::Io(e))? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    return Ok(());
                }

                // If editing in explorer or wizard input, handle typing
                if tui.editing_key.is_some() {
                    match key.code {
                        KeyCode::Enter => {
                            if let Some(k) = tui.editing_key.take() {
                                let val = tui.input_buffer.clone();
                                app.edit(&k, &val);
                            }
                        }
                        KeyCode::Esc => {
                            tui.editing_key = None;
                            tui.input_buffer.clear();
                        }
                        KeyCode::Backspace => {
                            tui.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            tui.input_buffer.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('1') => tui.current_tab = 0,
                    KeyCode::Char('2') => tui.current_tab = 1,
                    KeyCode::Char('3') => tui.current_tab = 2,
                    KeyCode::Char('4') => tui.current_tab = 3,
                    KeyCode::Char('5') => tui.current_tab = 4,
                    KeyCode::Char('6') => tui.current_tab = 5,
                    KeyCode::Tab => {
                        tui.current_tab = (tui.current_tab + 1) % titles.len();
                    }
                    KeyCode::BackTab => {
                        tui.current_tab = if tui.current_tab == 0 { titles.len() - 1 } else { tui.current_tab - 1 };
                    }
                    KeyCode::Char('p') => {
                        let next_idx = (Product::ALL.iter().position(|p| *p == app.product).unwrap_or(0) + 1) % Product::ALL.len();
                        app.select_product(next_idx);
                    }
                    KeyCode::Char('r') => app.refresh_status(),
                    KeyCode::Char('s') => app.refresh_sites(),
                    KeyCode::Char('v') => app.validate(),
                    KeyCode::Char('i') => app.init_env(),
                    KeyCode::Char('x') => app.toggle_reveal(),
                    KeyCode::Char('w') => app.save(),
                    _ => {
                        match tui.current_tab {
                            3 => {
                                match key.code {
                                    KeyCode::Down => {
                                        let i = match tui.env_table_state.selected() {
                                            Some(i) => if i + 1 < app.fields.len() { i + 1 } else { i },
                                            None => 0,
                                        };
                                        tui.env_table_state.select(Some(i));
                                    }
                                    KeyCode::Up => {
                                        let i = match tui.env_table_state.selected() {
                                            Some(i) => if i > 0 { i - 1 } else { 0 },
                                            None => 0,
                                        };
                                        tui.env_table_state.select(Some(i));
                                    }
                                    KeyCode::Enter => {
                                        if let Some(i) = tui.env_table_state.selected() {
                                            if let Some(field) = app.fields.get(i) {
                                                tui.editing_key = Some(field.key.clone());
                                                tui.input_buffer = field.value.clone();
                                            }
                                        }
                                    }
                                    KeyCode::Char('g') => {
                                        if let Some(i) = tui.env_table_state.selected() {
                                            if let Some(field) = app.fields.get(i) {
                                                let k = field.key.clone();
                                                app.generate(&k);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn render_dashboard(f: &mut ratatui::Frame, area: Rect, sidebar_area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    let info_text = format!(
        "Product: {:?} | Root: {} | Dirty: {} | Errors: {}",
        app.product,
        app.ops.root.display(),
        app.dirty,
        app.error_count()
    );
    let p = Paragraph::new(info_text)
        .block(Block::default().borders(Borders::ALL).title("Overview"))
        .style(Style::default().fg(Color::Green));
    f.render_widget(p, chunks[0]);

    let rows: Vec<Row> = app
        .findings
        .iter()
        .map(|f| {
            let sev_color = match f.severity {
                Severity::Error => Color::Red,
                Severity::Warn => Color::Yellow,
                Severity::Info => Color::Blue,
            };
            Row::new(vec![
                Cell::from(format!("{:?}", f.severity)).style(Style::default().fg(sev_color)),
                Cell::from(f.key.as_deref().unwrap_or("-")),
                Cell::from(f.message.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Length(10), Constraint::Length(20), Constraint::Min(30)],
    )
    .header(Row::new(vec!["Severity", "Key", "Message"]).style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title("Validation & Drift Findings"));
    f.render_widget(table, chunks[1]);

    let active_job = format!("Active Job: {:?}\nSites: {:?}\nShow Once: {:?}", app.active, app.sites, app.show_once);
    let sidebar = Paragraph::new(active_job)
        .block(Block::default().borders(Borders::ALL).title("Quick Info"))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(sidebar, sidebar_area);
}

fn render_devops(f: &mut ratatui::Frame, area: Rect, sidebar_area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    let p = Paragraph::new("DevOps Actions: [r] Refresh Status | Compose / Build / Release pipelines")
        .block(Block::default().borders(Borders::ALL).title("DevOps Control Center"));
    f.render_widget(p, chunks[0]);

    let rows: Vec<Row> = app
        .status
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s.service.clone()),
                Cell::from(s.state.clone()),
                Cell::from(s.health.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)],
    )
    .header(Row::new(vec!["Service", "State", "Health"]).style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title("Docker Compose Services (ps)"));
    f.render_widget(table, chunks[1]);

    let sidebar = Paragraph::new("Shortcuts:\n'r' - ps status\n's' - list sites\n'v' - validate\n'i' - init env\n'w' - save env")
        .block(Block::default().borders(Borders::ALL).title("DevOps Help"));
    f.render_widget(sidebar, sidebar_area);
}

fn render_monitor(f: &mut ratatui::Frame, area: Rect, sidebar_area: Rect, app: &AppState) {
    let rows: Vec<Row> = app
        .status
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s.service.clone()),
                Cell::from(s.state.clone()),
                Cell::from(s.health.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)],
    )
    .header(Row::new(vec!["Service", "State", "Health"]).style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title("System Monitor & Containers"));
    f.render_widget(table, area);

    let sidebar = Paragraph::new(format!("Product: {:?}\nStatus Count: {}", app.product, app.status.len()))
        .block(Block::default().borders(Borders::ALL).title("Metrics"));
    f.render_widget(sidebar, sidebar_area);
}

fn render_explorer(f: &mut ratatui::Frame, area: Rect, sidebar_area: Rect, app: &AppState, tui: &TuiState) {
    let rows: Vec<Row> = app
        .fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let is_selected = tui.env_table_state.selected() == Some(idx);
            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if field.has_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(field.key.clone()),
                Cell::from(field.display.clone()),
                Cell::from(if field.is_secret { "Yes" } else { "No" }),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(35), Constraint::Percentage(50), Constraint::Percentage(15)],
    )
    .header(Row::new(vec!["Key", "Value (display)", "Secret"]).style(Style::default().add_modifier(Modifier::BOLD)))
    .block(Block::default().borders(Borders::ALL).title("Data Explorer (.env Editor) - [Enter] edit, [g] generate secret, [x] reveal, [w] save"));
    f.render_widget(table, area);

    let sidebar_content = if let Some(k) = &tui.editing_key {
        format!("Editing KEY:\n{k}\n\nInput Value:\n{}\n\n[Enter] confirm\n[Esc] cancel", tui.input_buffer)
    } else {
        format!(
            "Reveal Secrets: {}\nDirty: {}\n\nShortcuts:\n[Enter] Edit item\n[g] Generate secret\n[x] Toggle reveal\n[w] Save .env\n[i] Init .env",
            app.reveal_secrets, app.dirty
        )
    };
    let sidebar = Paragraph::new(sidebar_content)
        .block(Block::default().borders(Borders::ALL).title("Editor Inspector"))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(sidebar, sidebar_area);
}

fn render_logs(f: &mut ratatui::Frame, area: Rect, sidebar_area: Rect, app: &AppState) {
    let items: Vec<ListItem> = app
        .log
        .iter()
        .rev()
        .take(50)
        .map(|l| {
            let style = if l.is_err { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Gray) };
            ListItem::new(Line::from(vec![Span::styled(l.text.clone(), style)]))
        })
        .collect();

    let logs_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Live Logs (Tail)"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(logs_list, area);

    let sidebar = Paragraph::new("Log buffer size up to 2000 lines.\nBackground jobs emit events here.")
        .block(Block::default().borders(Borders::ALL).title("Log Info"));
    f.render_widget(sidebar, sidebar_area);
}

fn render_wizard(f: &mut ratatui::Frame, area: Rect, sidebar_area: Rect, app: &AppState, _tui: &TuiState) {
    let text = format!(
        "Wizard Actions for Product {:?}:\n\n1. Sites Listed: {:?}\n2. Active Job: {:?}\n3. Show Once (Admin Password): {:?}\n\nPress 's' to refresh sites list.",
        app.product, app.sites, app.active, app.show_once
    );
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Setup & Deployment Wizard"));
    f.render_widget(p, area);

    let sidebar = Paragraph::new("Wizard handles:\n- Site creation\n- App installation\n- Database migration\n- Backup\n- Release pipeline")
        .block(Block::default().borders(Borders::ALL).title("Wizard Help"));
    f.render_widget(sidebar, sidebar_area);
}

fn main() {
    match run() {
        Ok(true) => std::process::exit(0),
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
