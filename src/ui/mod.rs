pub mod theme;
pub mod widgets;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{AppState, FocusPane, Modal, View};
use crate::ui::widgets::{containers, finops, modals, nodes};

pub fn draw(f: &mut Frame, state: &AppState) {
    let size = f.area();
    if size.width < 40 || size.height < 12 {
        let msg = Paragraph::new("Terminal muito pequeno. Aumente a janela.")
            .style(Style::default().fg(theme::YELLOW))
            .block(Block::default().borders(Borders::ALL).title("TUI"));
        f.render_widget(msg, size);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(size);

    draw_header(f, chunks[0], state);
    draw_body(f, chunks[1], state);
    draw_footer(f, chunks[2], state);

    match &state.modal {
        Modal::AddNode(form) => modals::draw_add_node(f, size, form),
        Modal::DropNode { confirm } => modals::draw_drop_node(f, size, state, *confirm),
        Modal::None => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, state: &AppState) {
    let titles = vec!["[1] Cluster & FinOps", "[2] Compose Stacks", "[3] Bridges & Plugins"];
    let selected = match state.view {
        View::ClusterFinOps => 0,
        View::ComposeStacks => 1,
        View::BridgesPlugins => 2,
    };
    let projected = state.projected_total();
    let budget_color = theme::budget_color(projected, state.budget);
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(40), Constraint::Percentage(30)])
        .split(area);

    let left = Paragraph::new(Line::from(vec![
        Span::styled("Kodoo Orchestrator ", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)),
        Span::styled("v0.1.0", Style::default().fg(theme::SUBTEXT)),
    ])).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Projeto"));
    f.render_widget(left, header[0]);

    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::default().fg(theme::SUBTEXT))
        .highlight_style(Style::default().fg(theme::BLUE).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Views"));
    f.render_widget(tabs, header[1]);

    let right = Paragraph::new(Line::from(vec![
        Span::styled(format!("${:.2}", projected), Style::default().fg(budget_color).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" / ${:.2} mo", state.budget), Style::default().fg(theme::TEXT)),
    ])).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("FinOps Global"));
    f.render_widget(right, header[2]);
}

fn draw_body(f: &mut Frame, area: Rect, state: &AppState) {
    if state.view != View::ClusterFinOps {
        let msg = match state.view {
            View::ComposeStacks => "Compose Stacks: visão consolidada em evolução. Use [1] para Cluster & FinOps.",
            View::BridgesPlugins => "Bridges & Plugins: integrações planejadas. Use [1] para Cluster & FinOps.",
            _ => "",
        };
        let p = Paragraph::new(msg)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme::TEXT))
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("View"));
        f.render_widget(p, area);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(cols[0]);
    nodes::draw_nodes(f, left[0], state, state.focus == FocusPane::Nodes);
    containers::draw_containers(f, left[1], state, state.focus == FocusPane::Containers);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(cols[1]);
    finops::draw_finops(f, right[0], state, state.focus == FocusPane::Detail);
    finops::draw_detail_tabs(f, right[1], state, state.focus == FocusPane::Detail);
}

fn draw_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let text = match state.modal {
        Modal::AddNode(_) => "[Tab] Campo | [Enter] Confirmar | [Esc] Cancelar",
        Modal::DropNode { .. } => "[Tab/←/→] Selecionar | [Enter] Confirmar | [Esc/N] Cancelar",
        Modal::None => "[Tab] Switch Pane | [↑/↓] Navigate | [1-4] Detail Tab | [a] Add Node | [d] Drop Node | [s] Simulate Load | [q] Quit",
    };
    let p = Paragraph::new(text)
        .style(Style::default().fg(theme::SUBTEXT))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Atalhos"));
    f.render_widget(p, area);
}
