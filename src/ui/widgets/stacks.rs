use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AppState, StackPanel};
use crate::ui::{theme, widgets::ops as opsw};

pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    let tabs = ratatui::widgets::Tabs::new(vec![
        "Visão geral",
        "Operações & Onboarding",
        "Apps do bench (repositórios)",
    ])
    .select(match state.stack_panel {
        StackPanel::Overview => 0,
        StackPanel::Operations => 1,
        StackPanel::AppRepos => 2,
    })
    .highlight_style(
        Style::default()
            .fg(theme::TEAL)
            .add_modifier(Modifier::BOLD),
    )
    .style(Style::default().fg(theme::SUBTEXT))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Compose Stacks"),
    );
    f.render_widget(tabs, chunks[0]);

    match state.stack_panel {
        StackPanel::Overview => draw_overview(f, chunks[1], state),
        StackPanel::Operations => opsw::draw(f, chunks[1], state),
        StackPanel::AppRepos => draw_apps(f, chunks[1], state),
    }
}

fn draw_overview(f: &mut Frame, area: Rect, state: &AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    let product = state.featured_product.label();
    let services = [
        "configurator",
        "backend",
        "websocket",
        "queue-short",
        "queue-long",
        "scheduler",
        "frontend (nginx 8080)",
        "db (mariadb)",
        "redis-cache",
        "redis-queue",
        "cloudflared",
    ];
    let items: Vec<ListItem> = services
        .iter()
        .map(|s| {
            ListItem::new(Line::from(vec![
                Span::styled("• ", Style::default().fg(theme::TEAL)),
                Span::styled(*s, Style::default().fg(theme::TEXT)),
            ]))
        })
        .collect();
    let left = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!("Stack — {product}")),
    );
    f.render_widget(left, cols[0]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(cols[1]);
    let yaml = Paragraph::new(state.featured_compose_preview())
        .style(Style::default().fg(theme::TEXT))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("docker-compose (base + produto)"),
        );
    f.render_widget(yaml, right_chunks[0]);

    let env = state.current_env();
    let (env_path, env_status, dirty) = if let Some(e) = env {
        (e.path_hint.clone(), e.last_action.clone(), e.dirty)
    } else {
        ("(n/d)".into(), "sem estado".into(), false)
    };
    let ops_line = if let Some(o) = state.current_ops() {
        format!("Operações: {} — {}", o.state.label(), o.last_action)
    } else {
        "Operações: n/d".into()
    };
    let info = vec![
        Line::from(vec![
            Span::styled("Produto: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(
                product,
                Style::default()
                    .fg(theme::MAUVE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Env: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(env_path, Style::default().fg(theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Estado .env: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(
                if dirty {
                    "alterações pendentes"
                } else {
                    "sincronizado"
                },
                Style::default().fg(if dirty { theme::YELLOW } else { theme::GREEN }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Última ação .env: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(env_status, Style::default().fg(theme::TEXT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(ops_line, Style::default().fg(theme::PEACH))),
        Line::from(Span::styled(
            "Use [Tab] para Operações/Onboarding e Apps do bench (repositórios).",
            Style::default().fg(theme::SUBTEXT),
        )),
    ];
    let info_p = Paragraph::new(info).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Contexto"),
    );
    f.render_widget(info_p, right_chunks[1]);
}

fn draw_apps(f: &mut Frame, area: Rect, state: &AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);
    let apps_err = state.apps_load_error();
    let apps = state.apps_for_featured();
    let items: Vec<ListItem> = if let Some(err) = &apps_err {
        vec![
            ListItem::new(Line::from(Span::styled(
                "Não foi possível ler apps.json",
                Style::default().fg(theme::RED).add_modifier(Modifier::BOLD),
            ))),
            ListItem::new(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(theme::YELLOW),
            ))),
            ListItem::new(Line::from(Span::styled(
                "O arquivo foi preservado. Corrija o JSON antes de adicionar ou remover apps.",
                Style::default().fg(theme::SUBTEXT),
            ))),
        ]
    } else if apps.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "Nenhum app listado em apps.json",
            Style::default().fg(theme::SUBTEXT),
        )))]
    } else {
        apps.iter()
            .enumerate()
            .map(|(i, (url, branch))| {
                let selected = i == state.apps_selected;
                let style = if selected {
                    Style::default()
                        .fg(theme::BLUE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                ListItem::new(vec![
                    Line::from(Span::styled(format!("repo: {url}"), style)),
                    Line::from(Span::styled(
                        format!("  ref:  {branch}"),
                        Style::default().fg(theme::SUBTEXT),
                    )),
                ])
            })
            .collect()
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::TEAL))
            .title("Apps do bench (products/*/apps.json) — [a] adicionar, [d] remover"),
    );
    f.render_widget(list, cols[0]);

    let mut hints = vec![
        Line::from(Span::styled("Contexto para adicionar apps no bench", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("• Cada app precisa de um repositório Git (URL) e de uma referência fixa (tag ou commit)."),
        Line::from("• Evite branches móveis (main/master/develop) para builds reprodutíveis."),
        Line::from("• O bench usa esses dados para instalar os apps no site/produto."),
        Line::from(""),
    ];
    if let Some(ops) = state.current_ops() {
        if !ops.last_action.is_empty() {
            hints.push(Line::from(vec![
                Span::styled("Última ação: ", Style::default().fg(theme::SUBTEXT)),
                Span::styled(ops.last_action.clone(), Style::default().fg(theme::PEACH)),
            ]));
            hints.push(Line::from(""));
        }
    }
    hints.push(Line::from(Span::styled(
        "Build do produto:",
        Style::default().fg(theme::TEAL),
    )));
    hints.push(Line::from(state.build_hints()));
    let p = Paragraph::new(hints).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Ajuda e build.env"),
    );
    f.render_widget(p, cols[1]);
}
