use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::ui::theme;

pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
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
            .border_style(Style::default().fg(theme::BLUE))
            .title(format!("Compose Stack — {product}")),
    );
    f.render_widget(left, cols[0]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(cols[1]);

    let preview = state.featured_compose_preview();
    let yaml = Paragraph::new(preview)
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
    let ops = state.current_ops();
    let ops_line = if let Some(o) = ops {
        format!("Operações: {} — {}", o.state.label(), o.last_action)
    } else {
        "Operações: n/d".into()
    };
    let info = vec![
        Line::from(vec![
            Span::styled("Produto em destaque: ", Style::default().fg(theme::SUBTEXT)),
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
        Line::from(vec![Span::styled(
            ops_line,
            Style::default().fg(theme::PEACH),
        )]),
        Line::from(Span::styled(
            "Use [4] para ciclo de vida do .env e [5] para ciclo de vida do stack.",
            Style::default().fg(theme::SUBTEXT),
        )),
    ];
    let info_p = Paragraph::new(info).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Contexto do Produto"),
    );
    f.render_widget(info_p, right_chunks[1]);
}
