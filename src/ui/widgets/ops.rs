use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::domain::StackState;
use crate::ui::theme;

pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(6)])
        .split(area);

    let Some(ops) = state.current_ops() else {
        f.render_widget(Paragraph::new("Sem estado de operações").block(Block::default().borders(Borders::ALL).title("Operações")), area);
        return;
    };

    let status_color = match ops.state {
        StackState::Running => theme::GREEN,
        StackState::Degraded => theme::YELLOW,
        StackState::Stopped => theme::RED,
        StackState::Starting | StackState::Stopping => theme::BLUE,
    };

    let header = vec![
        Line::from(vec![
            Span::styled("Produto: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(state.featured_product.label(), Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled("Estado do stack: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(ops.state.label(), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![Span::styled("Última ação: ", Style::default().fg(theme::SUBTEXT)), Span::styled(&ops.last_action, Style::default().fg(theme::TEXT))]),
        Line::from(Span::styled("Ciclo de vida via common/dc.sh <produto> (simulado com segurança nesta TUI)", Style::default().fg(theme::SUBTEXT))),
    ];
    f.render_widget(Paragraph::new(header).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Operações do Stack")), chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    let actions = [
        ("up", "[u] Subir stack (up -d)", "Inicia serviços do produto"),
        ("down", "[x] Parar stack (down)", "Para e remove containers do compose"),
        ("migrate", "[m] Migrar site", "Executa bench migrate via serviço ops"),
        ("status", "[k] Atualizar status", "Consulta estado atual do stack"),
        ("logs", "[o] Ver logs", "Consulta logs recentes do compose"),
    ];
    let items: Vec<ListItem> = actions.iter().enumerate().map(|(i, (_, title, desc))| {
        let selected = i == state.ops_selected_action;
        ListItem::new(vec![
            Line::from(Span::styled(*title, Style::default().fg(if selected { theme::BLUE } else { theme::TEAL }).add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }))),
            Line::from(Span::styled(format!("  {desc}"), Style::default().fg(theme::SUBTEXT))),
        ])
    }).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme::BLUE)).title("Ações (Enter executa)"));
    f.render_widget(list, body[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(body[1]);

    let mut plan = vec![Line::from(Span::styled("Comandos de referência", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)))];
    let p = state.featured_product.label();
    plan.push(Line::from(format!("• common/dc.sh {p} up -d")));
    plan.push(Line::from(format!("• common/dc.sh {p} down")));
    plan.push(Line::from(format!("• common/dc.sh {p} logs --tail=100")));
    plan.push(Line::from(format!("• common/dc.sh {p} run --rm ops bench --site <SITE> migrate")));
    plan.push(Line::from(""));
    plan.push(Line::from(Span::styled("A execução aqui é simulada para demonstrar o fluxo com segurança no ambiente atual.", Style::default().fg(theme::YELLOW))));
    f.render_widget(Paragraph::new(plan).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Plano / Referência")), right[0]);

    let log_items: Vec<ListItem> = if ops.log_lines.is_empty() {
        vec![ListItem::new("(sem eventos)")]
    } else {
        ops.log_lines.iter().rev().take(30).rev().map(|l| ListItem::new(Line::from(Span::styled(l.clone(), Style::default().fg(theme::TEXT))))).collect()
    };
    let logs = List::new(log_items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Eventos recentes"));
    f.render_widget(logs, right[1]);
}
