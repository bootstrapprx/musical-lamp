use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::AppState;
use crate::ui::theme;

pub fn draw_containers(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let costs = state.container_costs_map();
    let mut items: Vec<ListItem> = Vec::new();
    let selected_node_id = state.selected_node().map(|n| n.id.clone());
    let list_containers = state.containers_for_selected_node();
    for c in &list_containers {
        let cost = costs.get(&c.id).copied().unwrap_or(0.0);
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("• {} ", c.name), Style::default().fg(theme::TEAL)),
            Span::styled(format!("${:.2}/mo", cost), Style::default().fg(theme::YELLOW)),
            Span::styled(format!("  [{}]", c.status), Style::default().fg(theme::SUBTEXT)),
        ])));
    }
    if let Some(nid) = selected_node_id {
        let idle = costs.get(&format!("__idle__:{nid}")).copied().unwrap_or(0.0);
        items.push(ListItem::new(Line::from(vec![
            Span::styled("• idle / unallocated ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(format!("${:.2}/mo", idle), Style::default().fg(theme::PEACH)),
        ])));
    }
    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled("Nenhum container neste nó", Style::default().fg(theme::SUBTEXT)))));
    }

    let border_color = if focused { theme::BLUE } else { theme::OVERLAY };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title("CONTAINERS / STACKS");
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(theme::OVERLAY).add_modifier(Modifier::BOLD))
        .highlight_symbol("› ");
    let mut st = ListState::default();
    let max = list_containers.len().saturating_add(1);
    if max > 0 {
        st.select(Some(state.selected_container.min(max.saturating_sub(1))));
    }
    f.render_stateful_widget(list, area, &mut st);
}
