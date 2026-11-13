use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::domain::{PreflightStatus, StackState};
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
            Span::styled("Estado: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(ops.state.label(), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![Span::styled("Última ação: ", Style::default().fg(theme::SUBTEXT)), Span::styled(&ops.last_action, Style::default().fg(theme::TEXT))]),
        Line::from(Span::styled("Subir, parar, consultar, pré-checar e iniciar configuração (execução simulada com segurança).", Style::default().fg(theme::SUBTEXT))),
    ];
    f.render_widget(Paragraph::new(header).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Operações do Stack")), chunks[0]);

    let body = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(40), Constraint::Percentage(60)]).split(chunks[1]);

    let actions = [
        ("[u] Subir stack", "Inicia os serviços (up -d)"),
        ("[x] Parar stack", "Para e remove containers (down)"),
        ("[c] Consultar stack", "Consulta status e serviços"),
        ("[f] Pre-flight", "Valida .env, arquivos e prontidão"),
        ("[i] Onboarding", "Abre/fecha guia inicial"),
        ("[m] Migrar site", "bench --site <SITE> migrate (ops)"),
        ("[o] Ver logs", "Consulta logs recentes"),
    ];
    let items: Vec<ListItem> = actions.iter().enumerate().map(|(i, (title, desc))| {
        let selected = i == state.ops_selected_action;
        ListItem::new(vec![
            Line::from(Span::styled(*title, Style::default().fg(if selected { theme::BLUE } else { theme::TEAL }).add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }))),
            Line::from(Span::styled(format!("  {desc}"), Style::default().fg(theme::SUBTEXT))),
        ])
    }).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme::BLUE)).title("Ações (Enter executa)"));
    f.render_widget(list, body[0]);

    let right = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(55), Constraint::Percentage(45)]).split(body[1]);

    if state.show_onboarding {
        let mut lines = vec![Line::from(Span::styled("Onboarding — configuração inicial", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)))];
        for step in &state.onboarding_steps {
            let (mark, color) = if step.done { ("✔", theme::GREEN) } else { ("○", theme::YELLOW) };
            lines.push(Line::from(vec![Span::styled(format!("{mark} "), Style::default().fg(color)), Span::styled(step.title.clone(), Style::default().fg(if step.done { theme::TEXT } else { theme::PEACH }))]));
            lines.push(Line::from(Span::styled(format!("    {}", step.detail), Style::default().fg(theme::SUBTEXT))));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Onboarding ([i] fechar)")), right[0]);
    } else {
        let mut lines = vec![Line::from(Span::styled("Pre-flight — checagens de prontidão", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)))];
        if !state.preflight_ran {
            let p = state.featured_product.label();
            lines.push(Line::from(Span::styled("Nenhuma checagem executada ainda. Use [f] Pre-flight.", Style::default().fg(theme::SUBTEXT))));
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Ref: common/dc.sh {p} up -d / down / ps / logs")));
            lines.push(Line::from("Dica: configure .env em [3] Bridges & Plugins e apps do bench em 'Apps do bench'.")); 
        } else {
            for c in &state.preflight {
                let (mark, color) = match c.status { PreflightStatus::Ok => ("OK", theme::GREEN), PreflightStatus::Warn => ("AVISO", theme::YELLOW), PreflightStatus::Error => ("ERRO", theme::RED), PreflightStatus::Pending => ("...", theme::SUBTEXT) };
                lines.push(Line::from(vec![Span::styled(format!("[{mark}] "), Style::default().fg(color)), Span::styled(format!("{}: ", c.name), Style::default().fg(theme::TEXT)), Span::styled(c.detail.clone(), Style::default().fg(theme::SUBTEXT))]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Use [i] Onboarding para ver o passo a passo.", Style::default().fg(theme::SUBTEXT))));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Pre-flight / Referência")), right[0]);
    }

    let log_items: Vec<ListItem> = if ops.log_lines.is_empty() { vec![ListItem::new("(sem eventos)")] } else {
        ops.log_lines.iter().rev().take(30).rev().map(|l| ListItem::new(Line::from(Span::styled(l.clone(), Style::default().fg(theme::TEXT))))).collect()
    };
    let logs = List::new(log_items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Eventos / Consulta da stack"));
    f.render_widget(logs, right[1]);
}
