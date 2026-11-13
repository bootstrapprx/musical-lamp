use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::AppState;
use crate::ui::theme;

pub fn draw_nodes(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let items: Vec<ListItem> = state
        .nodes
        .iter()
        .map(|n| {
            let status = if n.online { "●" } else { "○" };
            let color = if n.online {
                theme::GREEN
            } else {
                theme::SUBTEXT
            };
            let line = Line::from(vec![
                Span::styled(format!("{status} "), Style::default().fg(color)),
                Span::styled(n.display_name(), Style::default().fg(theme::TEXT)),
                Span::raw("  "),
                Span::styled(
                    format!("${:.2}/mo", n.monthly_cost),
                    Style::default().fg(theme::PEACH),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();
    let border_color = if focused { theme::BLUE } else { theme::OVERLAY };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title("NODES - VPS Pool");
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(theme::OVERLAY)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut st = ListState::default();
    if !state.nodes.is_empty() {
        st.select(Some(
            state.selected_node.min(state.nodes.len().saturating_sub(1)),
        ));
    }
    f.render_stateful_widget(list, area, &mut st);
}
