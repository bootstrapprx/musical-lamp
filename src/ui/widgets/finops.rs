use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Sparkline, Tabs, Wrap};
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
    let cpu_line = Line::from(vec![
        Span::styled("CPU ", Style::default().fg(theme::SUBTEXT)),
        Span::styled(
            braille_bar(node.cpu_pct, chunks[0].width.saturating_sub(12) as usize),
            Style::default().fg(cpu_color),
        ),
        Span::styled(
            format!(" {:>5.1}%", node.cpu_pct),
            Style::default().fg(cpu_color),
        ),
    ]);
    f.render_widget(Paragraph::new(cpu_line), chunks[0]);

    let ram_color = theme::health_color(node.ram_pct);
    let ram_line = Line::from(vec![
        Span::styled("RAM ", Style::default().fg(theme::SUBTEXT)),
        Span::styled(
            braille_bar(node.ram_pct, chunks[1].width.saturating_sub(12) as usize),
            Style::default().fg(ram_color),
        ),
        Span::styled(
            format!(" {:>5.1}%", node.ram_pct),
            Style::default().fg(ram_color),
        ),
    ]);
    f.render_widget(Paragraph::new(ram_line), chunks[1]);

    let spark_data: Vec<u64> = node.cpu_history.to_vec();
    let spark = Sparkline::default()
        .block(Block::default().title("Consumo recente (CPU/RAM)"))
        .data(&spark_data)
        .style(Style::default().fg(theme::TEAL));
    f.render_widget(spark, chunks[2]);

    let (trend_symbol, trend_pct) = trend_indicator(&node.cpu_history, &node.ram_history);
    let eff = efficiency_score(node.cpu_pct, node.ram_pct);
    let proj = projected_30d(node.monthly_cost, node.cpu_pct, node.ram_pct, trend_pct);
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
    let trend_label = format!("{trend_symbol} {:+.1}%", trend_pct);
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "Custo Atual Estimado: ",
                Style::default().fg(theme::SUBTEXT),
            ),
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
                format!("${:.2}/mo {}", proj.0, trend_label),
                Style::default().fg(if trend_pct >= 0.0 {
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
                if node.online {
                    "online"
                } else {
                    "offline/standby"
                }
            )),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), chunks[3]);
}

fn braille_bar(pct: f64, width: usize) -> String {
    let width = width.clamp(8, 40);
    let levels = [' ', '⡀', '⡄', '⡆', '⡇', '⣇', '⣧', '⣷', '⣿'];
    let total_steps = width * 8;
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * total_steps as f64).round() as usize;
    let mut out = String::with_capacity(width);
    for i in 0..width {
        let cell = filled.saturating_sub(i * 8).min(8);
        out.push(levels[cell]);
    }
    out
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

fn trend_indicator(cpu: &[u64], ram: &[u64]) -> (&'static str, f64) {
    let avg = |h: &[u64]| -> f64 {
        if h.is_empty() {
            0.0
        } else {
            h.iter().map(|v| *v as f64).sum::<f64>() / h.len() as f64
        }
    };
    let tail = |h: &[u64], n: usize| -> f64 {
        let start = h.len().saturating_sub(n);
        avg(&h[start..])
    };
    let recent = (tail(cpu, 3) + tail(ram, 3)) / 2.0;
    let older = (tail(cpu, 8) + tail(ram, 8)) / 2.0;
    let delta = (recent - older).clamp(-30.0, 30.0);
    let symbol = if delta > 0.8 {
        "↗"
    } else if delta < -0.8 {
        "↘"
    } else {
        "→"
    };
    (symbol, delta)
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
    format!("{score} ({util:.0}% utilização)")
}

fn projected_30d(base: f64, cpu: f64, ram: f64, trend_pct: f64) -> (f64, f64) {
    let pressure = ((cpu + ram) / 200.0 - 0.45).clamp(-0.2, 0.35);
    let trend_factor = (trend_pct / 100.0).clamp(-0.15, 0.2);
    let proj = base * (1.0 + pressure + trend_factor);
    (
        (proj * 100.0).round() / 100.0,
        (pressure + trend_factor) * 100.0,
    )
}
