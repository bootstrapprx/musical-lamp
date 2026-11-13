use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AddField, AddNodeForm, AppState};
use crate::ui::theme;

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn draw_add_node(f: &mut Frame, area: Rect, form: &AddNodeForm) {
    let area = centered_rect(60, 60, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .title("Add Node [a]")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BLUE));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), Constraint::Length(2), Constraint::Length(2),
            Constraint::Length(2), Constraint::Length(2), Constraint::Min(2),
        ])
        .split(inner);

    draw_field(f, rows[0], "Nome do Nó", &form.name, form.field == AddField::Name);
    draw_field(f, rows[1], "IP/Host", &form.host, form.field == AddField::Host);
    draw_field(f, rows[2], "Usuário SSH", &form.user, form.field == AddField::User);
    draw_field(f, rows[3], "Provedor", &form.provider, form.field == AddField::Provider);
    draw_field(f, rows[4], "Custo Mensal ($)", &form.cost, form.field == AddField::Cost);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)),
        Span::raw(" alterna campos | "),
        Span::styled("Enter", Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD)),
        Span::raw(" adiciona nó e recalcula orçamento | "),
        Span::styled("Esc", Style::default().fg(theme::RED).add_modifier(Modifier::BOLD)),
        Span::raw(" cancela"),
    ])).style(Style::default().fg(theme::SUBTEXT)).wrap(Wrap { trim: true });
    f.render_widget(hint, rows[5]);
}

fn draw_field(f: &mut Frame, area: Rect, label: &str, value: &str, active: bool) {
    let border = if active { theme::BLUE } else { theme::OVERLAY };
    let title = if active { format!("{label} (ativo)") } else { label.to_string() };
    let p = Paragraph::new(value.to_string())
        .style(Style::default().fg(if active { theme::TEXT } else { theme::SUBTEXT }))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(border)).title(title));
    f.render_widget(p, area);
}

pub fn draw_drop_node(f: &mut Frame, area: Rect, state: &AppState, confirm: bool) {
    let area = centered_rect(55, 40, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .title("Drop Node [d]")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::RED));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (name, cost, count) = if let Some(n) = state.selected_node() {
        let c = state.containers.iter().filter(|c| c.node_id == n.id).count();
        (n.display_name(), n.monthly_cost, c)
    } else {
        ("(nenhum nó)".into(), 0.0, 0)
    };
    let after = (state.projected_total() - cost).max(0.0);
    let msg = vec![
        Line::from(Span::styled("Confirmar remoção segura do nó selecionado?", Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD))),
        Line::from(format!("Nó: {name}  |  Custo fixo: ${cost:.2}/mo")),
        Line::from(format!("Containers que precisarão realocação: {count}")),
        Line::from(format!("Impacto: orçamento projetado {before:.2} -> {after:.2} /mo", before = state.projected_total())),
        Line::from(""),
        Line::from(vec![
            Span::styled(if confirm { "[ Confirmar ]" } else { "  Confirmar  " }, Style::default().fg(if confirm { theme::BASE } else { theme::GREEN }).bg(if confirm { theme::GREEN } else { theme::MANTLE })),
            Span::raw("   "),
            Span::styled(if !confirm { "[ Cancelar ]" } else { "  Cancelar  " }, Style::default().fg(if !confirm { theme::BASE } else { theme::SUBTEXT }).bg(if !confirm { theme::YELLOW } else { theme::MANTLE })),
        ]),
        Line::from(Span::styled("Tab/←/→ alterna, Enter confirma, Esc cancela", Style::default().fg(theme::SUBTEXT))),
    ];
    f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), inner);
}
