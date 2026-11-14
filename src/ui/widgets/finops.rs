use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph, Sparkline, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{AppState, DetailTab};
use crate::ui::theme;

pub fn draw_finops(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused { theme::BLUE } else { theme::OVERLAY };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title("FINOPS & TELEMETRY");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let Some(node) = state.selected_node() else {
        f.render_widget(
            Paragraph::new("Selecione um nó").style(Style::default().fg(theme::SUBTEXT)),
            inner,
        );
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .split(inner);

    let cpu_color = theme::health_color(node.cpu_pct);
    let cpu = Gauge::default()
        .gauge_style(Style::default().fg(cpu_color).bg(theme::MANTLE))
        .label(format!("CPU {:>5.1}%", node.cpu_pct))
        .use_unicode(true)
        .ratio((node.cpu_pct / 100.0).clamp(0.0, 1.0));
    f.render_widget(cpu, chunks[0]);

    let ram_color = theme::health_color(node.ram_pct);
    let ram = Gauge::default()
        .gauge_style(Style::default().fg(ram_color).bg(theme::MANTLE))
        .label(format!("RAM {:>5.1}%", node.ram_pct))
        .use_unicode(true)
        .ratio((node.ram_pct / 100.0).clamp(0.0, 1.0));
    f.render_widget(ram, chunks[1]);

    let spark_data: Vec<u64> = node.cpu_history.to_vec();
    let spark = Sparkline::default()
        .block(Block::default().title("CPU recente"))
        .data(&spark_data)
        .style(Style::default().fg(theme::TEAL));
    f.render_widget(spark, chunks[2]);

    let trend = trend_arrow(&node.cpu_history);
    let eff = efficiency_score(node.cpu_pct, node.ram_pct);
    let proj = projected_30d(node.monthly_cost, node.cpu_pct, node.ram_pct);
    let costs = state.container_costs_map();
    let allocated: f64 = state
        .containers_for_selected_node()
        .iter()
        .map(|c| costs.get(&c.id).copied().unwrap_or(0.0))
        .sum();
    let pct_node = if node.monthly_cost > 0.0 {
        (allocated / node.monthly_cost * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("Custo Atual Estimado: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(
                format!("${:.2}/mo", allocated),
                Style::default()
                    .fg(theme::PEACH)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  ({:.1}% do nó)", pct_node)),
        ]),
        Line::from(vec![
            Span::styled("Projeção de 30 dias: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(
                format!("${:.2}/mo {trend}", proj.0),
                Style::default().fg(if proj.1 >= 0.0 {
                    theme::YELLOW
                } else {
                    theme::GREEN
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Score de Eficiência: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(eff, Style::default().fg(theme::TEXT)),
            Span::raw(format!(
                "  | Nó: {} ({})",
                node.name,
                if node.online { "online" } else { "offline" }
            )),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), chunks[3]);
}

pub fn draw_detail_tabs(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let border_color = if focused { theme::BLUE } else { theme::OVERLAY };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title("Detalhes");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);

    let titles = vec![
        "[Logs]",
        "[Cost Breakdown]",
        "[Compose YAML]",
        "[Forecast 7d]",
    ];
    let idx = match state.detail_tab {
        DetailTab::Logs => 0,
        DetailTab::CostBreakdown => 1,
        DetailTab::ComposeYaml => 2,
        DetailTab::Forecast7d => 3,
    };
    let tabs = Tabs::new(titles)
        .select(idx)
        .highlight_style(
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(theme::SUBTEXT));
    f.render_widget(tabs, chunks[0]);

    match state.detail_tab {
        DetailTab::Logs => draw_logs(f, chunks[1], state),
        DetailTab::CostBreakdown => draw_cost_breakdown(f, chunks[1], state),
        DetailTab::ComposeYaml => draw_compose(f, chunks[1], state),
        DetailTab::Forecast7d => draw_forecast(f, chunks[1], state),
    }
}

fn draw_logs(f: &mut Frame, area: Rect, state: &AppState) {
    let text = if let Some(c) = state.selected_container_obj() {
        if c.logs.is_empty() {
            "(sem logs ainda)".to_string()
        } else {
            c.logs.join("\n")
        }
    } else {
        "Selecione um container para ver logs.".to_string()
    };
    let p = Paragraph::new(text)
        .style(Style::default().fg(theme::TEXT))
        .wrap(Wrap { trim: false })
        .scroll((state.log_scroll, 0));
    f.render_widget(p, area);
}

fn draw_cost_breakdown(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(node) = state.selected_node() else {
        return;
    };
    let costs = state.container_costs_map();
    let mut lines = vec![
        Line::from(Span::styled(
            "Fórmula de rateio: CPU share + RAM share -> custo ($/mês) por container",
            Style::default().fg(theme::SUBTEXT),
        )),
        Line::from(Span::styled(
            "custo_container = custo_nó * ((CPU% + RAM%) / 2 / 100)",
            Style::default().fg(theme::SUBTEXT),
        )),
        Line::from(Span::raw("")),
    ];
    for c in state.containers_for_selected_node() {
        let cost = costs.get(&c.id).copied().unwrap_or(0.0);
        let impact = if c.cpu_share_pct + c.ram_share_pct > 120.0 {
            "alto impacto ao escalar"
        } else {
            "movimentação viável"
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<18}", c.name), Style::default().fg(theme::TEAL)),
            Span::styled(
                format!(" ${:>6.2}/mo", cost),
                Style::default().fg(theme::YELLOW),
            ),
            Span::raw(format!(
                "  CPU {:.1}% RAM {:.1}%  | {}",
                c.cpu_share_pct, c.ram_share_pct, impact
            )),
        ]));
    }
    let idle = costs
        .get(&format!("__idle__:{}", node.id))
        .copied()
        .unwrap_or(0.0);
    lines.push(Line::from(vec![
        Span::styled(
            "idle / unallocated ".to_string(),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled(
            format!("${:.2}/mo", idle),
            Style::default().fg(theme::PEACH),
        ),
        Span::raw("  (capacidade paga ociosa)"),
    ]));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_compose(f: &mut Frame, area: Rect, state: &AppState) {
    let y = state.compose_yaml_for_selected();
    f.render_widget(
        Paragraph::new(y)
            .style(Style::default().fg(theme::TEXT))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_forecast(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(node) = state.selected_node() else {
        return;
    };
    let trend = if node.cpu_history.len() >= 2 {
        *node.cpu_history.last().unwrap_or(&0) as f64
            - *node.cpu_history.first().unwrap_or(&0) as f64
    } else {
        0.0
    };
    let mut lines = vec![Line::from(Span::styled(
        "Projeção 7 dias (estimativa por tendência recente)",
        Style::default().fg(theme::SUBTEXT),
    ))];
    for day in 1..=7 {
        let est = (node.cpu_pct + trend * 0.05 * day as f64).clamp(0.0, 120.0);
        let warn = if est >= 90.0 {
            " ⚠ gargalo provável"
        } else if est >= 75.0 {
            " atenção"
        } else {
            ""
        };
        let bar_len = (est / 5.0).round() as usize;
        let bar = "█".repeat(bar_len.min(20));
        lines.push(Line::from(vec![
            Span::styled(format!("D+{day}: "), Style::default().fg(theme::TEXT)),
            Span::styled(
                format!("{:>5.1}% ", est.min(100.0)),
                Style::default().fg(theme::health_color(est.min(100.0))),
            ),
            Span::styled(bar, Style::default().fg(theme::BLUE)),
            Span::styled(warn.to_string(), Style::default().fg(theme::YELLOW)),
        ]));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn trend_arrow(history: &[u64]) -> &'static str {
    if history.len() < 2 {
        return "→ 0%";
    }
    let a = history[history.len().saturating_sub(2)] as f64;
    let b = *history.last().unwrap_or(&0) as f64;
    if b > a + 1.0 {
        "↗ +%"
    } else if b + 1.0 < a {
        "↘ -%"
    } else {
        "→ estável"
    }
}

fn efficiency_score(cpu: f64, ram: f64) -> String {
    let util = (cpu + ram) / 2.0;
    let score = if util < 20.0 {
        "Baixa (ociosidade)"
    } else if util < 70.0 {
        "Boa"
    } else if util < 90.0 {
        "Alta pressão"
    } else {
        "Crítica"
    };
    format!("{score} ({util:.0}%)")
}

fn projected_30d(base: f64, cpu: f64, ram: f64) -> (f64, f64) {
    let pressure = ((cpu + ram) / 200.0 - 0.5).clamp(-0.2, 0.3);
    let proj = base * (1.0 + pressure);
    ((proj * 100.0).round() / 100.0, pressure * 100.0)
}
