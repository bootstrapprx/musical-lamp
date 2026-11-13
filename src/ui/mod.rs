pub mod theme;
pub mod widgets;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{AppState, FocusPane, Modal, View};
use crate::ui::widgets::{containers, env as envw, finops, modals, nodes, ops as opsw, stacks};

pub fn draw(f: &mut Frame, state: &AppState) {
    let size = f.area();
    if size.width < 50 || size.height < 14 {
        let msg = Paragraph::new("Terminal muito pequeno. Aumente a janela para usar a TUI.")
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
        Modal::TunnelToken { input, product } => modals::draw_tunnel_token(f, size, *product, input),
        Modal::None => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, state: &AppState) {
    let titles = vec![
        "[1] Cluster & FinOps",
        "[2] Compose Stacks",
        "[3] Bridges & Plugins",
        "[4] .env",
        "[5] Operações",
    ];
    let selected = match state.view {
        View::ClusterFinOps => 0,
        View::ComposeStacks => 1,
        View::BridgesPlugins => 2,
        View::EnvLifecycle => 3,
        View::Operations => 4,
    };
    let projected = state.projected_total();
    let budget_color = theme::budget_color(projected, state.budget);
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Percentage(44), Constraint::Percentage(28)])
        .split(area);

    let left = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Kodoo Orchestrator ", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)),
            Span::styled("v0.1.0", Style::default().fg(theme::SUBTEXT)),
        ]),
        Line::from(vec![
            Span::styled("Produto: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(state.featured_product.label(), Style::default().fg(theme::TEAL).add_modifier(Modifier::BOLD)),
            Span::styled("  [p] alternar", Style::default().fg(theme::OVERLAY)),
        ]),
    ]).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Projeto"));
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
    match state.view {
        View::ClusterFinOps => draw_cluster(f, area, state),
        View::ComposeStacks => stacks::draw(f, area, state),
        View::BridgesPlugins => {
            let has_token = crate::app::env_has_tunnel_token(state.featured_product);
            let token_status = if has_token { "configurado" } else { "ausente (necessário para subir o cloudflared)" };
            let token_color = if has_token { theme::GREEN } else { theme::YELLOW };
            let lines = vec![
                Line::from(Span::styled("Cloudflare Tunnel (container do compose)", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD))),
                Line::from("• Serviço: cloudflared (imagem cloudflare/cloudflared)"),
                Line::from("• Execução: via Docker Compose (common/compose.base.yaml), rede edge isolada e IP fixo"),
                Line::from(vec![
                    Span::raw("• TUNNEL_TOKEN: "),
                    Span::styled(token_status, Style::default().fg(token_color)),
                ]),
                Line::from(""),
                Line::from("Ao abrir esta aba, se o token não existir para o produto em destaque, a TUI solicita o preenchimento."),
                Line::from("Você também pode gerar/editar/salvar o .env na aba [4] (variável TUNNEL_TOKEN)."),
                Line::from(""),
                Line::from(Span::styled("Outras bridges/plugins: integrações adicionais planejadas.", Style::default().fg(theme::SUBTEXT))),
            ];
            let p = Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(theme::TEXT))
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Bridges & Plugins"));
            f.render_widget(p, area);
        }
        View::EnvLifecycle => envw::draw(f, area, state),
        View::Operations => opsw::draw(f, area, state),
    }
}

fn draw_cluster(f: &mut Frame, area: Rect, state: &AppState) {
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
        Modal::TunnelToken { .. } => "[Enter] Salvar token | [Esc] Pular por agora",
        Modal::None => match state.view {
            View::ClusterFinOps => "[Tab] Painel | [↑/↓] Navegar | [1-4] Aba detalhe | [a] Add Nó | [d] Drop Nó | [s] Simular carga | [p] Produto | [1-5] Views | [q] Sair",
            View::ComposeStacks => "[↑/↓] Rolar | [p] Produto | [1-5] Views | [q] Sair",
            View::BridgesPlugins => "[p] Produto | [1-5] Views | [q] Sair",
            View::EnvLifecycle => if state.env_editing { "[Enter] Salvar valor | [Esc] Cancelar edição | (digitando)" } else { "[↑/↓] Variável | [Enter] Editar | [g] Gerar de exemplo | [r] Recarregar | [w] Salvar .env | [p] Produto | [1-5] Views | [q] Sair" },
            View::Operations => "[↑/↓/←/→] Ação | [Enter] Executar | [u] Up | [x] Down | [m] Migrar | [k] Status | [o] Logs | [p] Produto | [1-5] Views | [q] Sair",
        },
    };
    let p = Paragraph::new(text)
        .style(Style::default().fg(theme::SUBTEXT))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Atalhos"));
    f.render_widget(p, area);
}
