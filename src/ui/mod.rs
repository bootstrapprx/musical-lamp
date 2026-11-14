pub mod layout;
pub mod theme;
pub mod widgets;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{AppState, BridgePanel, FocusPane, Modal, StackPanel, View};
use crate::ui::widgets::{containers, env as envw, finops, modals, nodes, stacks};

pub fn draw(f: &mut Frame, state: &AppState) {
    let size = f.area();
    if size.width < 50 || size.height < 18 {
        let msg = Paragraph::new(
            "Terminal muito pequeno. Aumente a janela (mínimo 50x18) para usar a TUI.",
        )
        .style(Style::default().fg(theme::YELLOW))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("TUI"),
        );
        f.render_widget(msg, size);
        return;
    }

    let shell = layout::app_shell(size);
    draw_header(f, shell[0], state);
    draw_body(f, shell[1], state);
    draw_footer(f, shell[2], state);

    match &state.modal {
        Modal::AddNode(form) => modals::draw_add_node(f, size, form),
        Modal::DropNode { confirm } => modals::draw_drop_node(f, size, state, *confirm),
        Modal::AddApp(form) => modals::draw_add_app(f, size, form),
        Modal::None => {}
    }
}

fn draw_header(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let cols = layout::header_cols(area);
    let left = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "musical-lamp-tui",
                Style::default()
                    .fg(theme::MAUVE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  v0.1.0", Style::default().fg(theme::SUBTEXT)),
        ]),
        Line::from(Span::styled(
            "Orquestrador SaaS • Docker Compose • FinOps",
            Style::default().fg(theme::OVERLAY),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Projeto"),
    );
    f.render_widget(left, cols[0]);

    let titles = vec![
        "[1] Cluster & FinOps",
        "[2] Compose Stacks",
        "[3] Bridges & Plugins",
    ];
    let selected = match state.view {
        View::ClusterFinOps => 0,
        View::ComposeStacks => 1,
        View::BridgesPlugins => 2,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .style(Style::default().fg(theme::SUBTEXT))
        .highlight_style(
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Views"),
        );
    f.render_widget(tabs, cols[1]);

    let projected = state.projected_total();
    let budget_color = theme::budget_color(projected, state.budget);
    let right = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                format!("${:.2}", projected),
                Style::default()
                    .fg(budget_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" / ${:.2} mo", state.budget),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(Span::styled(
            "Custo mensal projetado total vs orçamento",
            Style::default().fg(theme::SUBTEXT),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Resumo financeiro global"),
    );
    f.render_widget(right, cols[2]);
}

fn draw_body(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    match state.view {
        View::ClusterFinOps => draw_cluster(f, area, state),
        View::ComposeStacks => stacks::draw(f, area, state),
        View::BridgesPlugins => draw_bridges(f, area, state),
    }
}

fn draw_cluster(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let cols = layout::master_detail(area);
    let left = layout::left_stack(cols[0]);
    let right = layout::right_stack(cols[1]);
    nodes::draw_nodes(f, left[0], state, state.focus == FocusPane::Nodes);
    containers::draw_containers(f, left[1], state, state.focus == FocusPane::Containers);
    finops::draw_finops(f, right[0], state, state.focus == FocusPane::Detail);
    finops::draw_detail_tabs(f, right[1], state, state.focus == FocusPane::Detail);
}

fn draw_bridges(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    match state.bridge_panel {
        BridgePanel::Env => envw::draw(f, area, state),
        BridgePanel::Tunnel => {
            let has_token = crate::app::env_has_tunnel_token(state.featured_product);
            let token_status = if has_token { "configurado" } else { "ausente" };
            let token_color = if has_token {
                theme::GREEN
            } else {
                theme::YELLOW
            };
            let lines = vec![
                Line::from(Span::styled("Bridges & Plugins", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(vec![Span::styled("Cloudflare Tunnel (cloudflared): ", Style::default().fg(theme::TEXT)), Span::styled(token_status, Style::default().fg(token_color))]),
                Line::from("• Serviço executado via Docker Compose com cloudflare/cloudflared:latest."),
                Line::from("• Comando: tunnel --no-autoupdate run --token \"$TUNNEL_TOKEN\" (token definido no .env)."),
                Line::from("• Configure TUNNEL_TOKEN no painel [Tab] Env. Campos de rede (EDGE_SUBNET/EDGE_IP_RANGE/CLOUDFLARED_IP) devem ser IPv4/CIDR coerentes."),
                Line::from(""),
                Line::from(Span::styled("Dica: use [Tab] para alternar entre Env e Tunnel. Em Env, [n] gera segredos e os campos mostram contexto (URL, caminho, imagem).", Style::default().fg(theme::SUBTEXT))),
            ];
            let p = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Bridges & Plugins — Tunnel"),
            );
            f.render_widget(p, area);
        }
    }
}

fn draw_footer(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let text = match &state.modal {
        Modal::AddNode(_) => "[Tab] Campo | [Enter] Confirmar | [Esc] Cancelar",
        Modal::DropNode { .. } => "[Tab/←/→] Alternar | [Enter/y] Confirmar | [Esc/n] Cancelar",
        Modal::AddApp(form) => {
            if form.field == crate::app::AppField::Ref {
                "[↑/↓] Selecionar ref | [Espaço/Enter] Abrir/confirmar | [Ctrl+n] Nova ref | [Tab] Campo | [Esc] Fechar"
            } else {
                "[Tab] Campo (URL/Ref) | [Enter] Adicionar | [Esc] Cancelar"
            }
        }
        Modal::None => match state.view {
            View::ClusterFinOps => {
                "[Tab] Switch Pane | [↑/↓] Navigate | [1-4] Detail Tab | [a] Add Node | [d] Drop Node | [s] Simulate Load | [q] Quit"
            }
            View::ComposeStacks => match state.stack_panel {
                StackPanel::Overview => "[Tab] Operações/Repositórios | [p] Produto | [1-3] Views | [q] Quit",
                StackPanel::Operations => {
                    "[↑/↓] Ação | [Enter] Executar | [u] Subir | [x] Parar | [c] Consultar | [f] Pre-flight | [k] Doctor | [K] Corrigir | [m] Migrar | [o] Logs | [Tab] Próximo painel | [q] Quit"
                }
                StackPanel::AppRepos => "[↑/↓] Selecionar | [a] Adicionar | [d] Remover | [Tab] Próximo painel | [q] Quit",
            },
            View::BridgesPlugins => match state.bridge_panel {
                BridgePanel::Env => {
                    if state.env_editing {
                        "[Enter] Salvar valor | [Esc] Cancelar"
                    } else {
                        "[↑/↓] Variável | [Enter] Editar | [g] Gerar do exemplo | [w] Salvar .env | [Tab] Tunnel | [1-3] Views | [q] Quit"
                    }
                }
                BridgePanel::Tunnel => "[Tab] Env | [1-3] Views | [q] Quit",
            },
        },
    };
    let p = Paragraph::new(text)
        .style(Style::default().fg(theme::SUBTEXT))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Atalhos"),
        );
    f.render_widget(p, area);
}
