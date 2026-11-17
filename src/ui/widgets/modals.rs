use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{
    AddAppForm, AddField, AddNodeForm, AppField, AppState, NewProductField, NewProductForm,
    RefMode, ResetEnvForm, ResetEnvMode,
};
use crate::ui::{layout, theme};

pub fn draw_add_node(f: &mut Frame, area: Rect, form: &AddNodeForm) {
    let area = layout::centered_rect(60, 50, area);
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::MANTLE)),
        area,
    );
    let block = Block::default()
        .title("Add Node [a]")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BLUE));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let compact = inner.height < 13;
    let field_h = if compact { 1 } else { 2 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(field_h),
            Constraint::Length(field_h),
            Constraint::Length(field_h),
            Constraint::Length(field_h),
            Constraint::Length(field_h),
            Constraint::Min(1),
        ])
        .split(inner);

    draw_field(
        f,
        rows[0],
        "Nome do Nó",
        &form.name,
        form.field == AddField::Name,
    );
    draw_field(
        f,
        rows[1],
        "IP/Host",
        &form.host,
        form.field == AddField::Host,
    );
    draw_field(
        f,
        rows[2],
        "Usuário SSH",
        &form.user,
        form.field == AddField::User,
    );
    draw_field(
        f,
        rows[3],
        "Provedor",
        &form.provider,
        form.field == AddField::Provider,
    );
    draw_field(
        f,
        rows[4],
        "Custo Mensal ($)",
        &form.cost,
        form.field == AddField::Cost,
    );
    let hint_text = if compact {
        "Tab: campo | Enter: confirma | Esc: cancela"
    } else {
        "Tab alterna campos | Enter adiciona nó e recalcula orçamento | Esc cancela"
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(theme::SUBTEXT))
        .wrap(Wrap { trim: true });
    f.render_widget(hint, rows[5]);
}

fn draw_field(f: &mut Frame, area: Rect, label: &str, value: &str, active: bool) {
    let border = if active { theme::BLUE } else { theme::OVERLAY };
    let title = if active {
        format!("{label} (ativo)")
    } else {
        label.to_string()
    };
    if area.height <= 1 {
        let line = Line::from(vec![
            Span::styled(
                format!("{label}: "),
                Style::default().fg(border).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                value.to_string(),
                Style::default().fg(if active { theme::TEXT } else { theme::SUBTEXT }),
            ),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let p = Paragraph::new(value.to_string())
        .style(Style::default().fg(if active { theme::TEXT } else { theme::SUBTEXT }))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border))
                .title(title),
        );
    f.render_widget(p, area);
}

pub fn draw_drop_node(f: &mut Frame, area: Rect, state: &AppState, confirm: bool) {
    let area = layout::centered_rect(60, 48, area);
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::MANTLE)),
        area,
    );
    let block = Block::default()
        .title("Drop Node [d]")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::RED));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (name, cost, to_move): (String, f64, Vec<String>) = if let Some(n) = state.selected_node() {
        let names = state
            .containers
            .iter()
            .filter(|c| c.node_id == n.id)
            .map(|c| c.name.clone())
            .collect();
        (n.display_name(), n.monthly_cost, names)
    } else {
        ("(nenhum nó)".into(), 0.0, vec![])
    };
    let after = (state.projected_total() - cost).max(0.0);
    let mut msg = vec![
        Line::from(Span::styled(
            "Confirmar remoção segura do nó selecionado?",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("Nó: {name}  |  Custo fixo: ${cost:.2}/mo")),
        Line::from(format!(
            "Impacto: orçamento projetado ${:.2} -> ${:.2}/mo",
            state.projected_total(),
            after
        )),
        Line::from(Span::styled(
            "Containers que precisarão ser realocados antes da exclusão:",
            Style::default().fg(theme::SUBTEXT),
        )),
    ];
    if to_move.is_empty() {
        msg.push(Line::from(Span::styled(
            "  • nenhum container ativo neste nó",
            Style::default().fg(theme::GREEN),
        )));
    } else {
        for c in to_move.iter().take(6) {
            msg.push(Line::from(Span::styled(
                format!("  • {c}"),
                Style::default().fg(theme::YELLOW),
            )));
        }
        if to_move.len() > 6 {
            msg.push(Line::from(Span::styled(
                format!("  • … +{} outros", to_move.len() - 6),
                Style::default().fg(theme::SUBTEXT),
            )));
        }
    }
    msg.push(Line::from(""));
    msg.push(Line::from(vec![
        Span::styled(
            if confirm {
                "[ Confirmar ]"
            } else {
                "  Confirmar  "
            },
            Style::default()
                .fg(if confirm { theme::BASE } else { theme::GREEN })
                .bg(if confirm { theme::GREEN } else { theme::MANTLE }),
        ),
        Span::raw("   "),
        Span::styled(
            if !confirm {
                "[ Cancelar ]"
            } else {
                "  Cancelar  "
            },
            Style::default()
                .fg(if !confirm {
                    theme::BASE
                } else {
                    theme::SUBTEXT
                })
                .bg(if !confirm {
                    theme::YELLOW
                } else {
                    theme::MANTLE
                }),
        ),
    ]));
    msg.push(Line::from(Span::styled(
        "Tab/←/→ alterna, Enter/y confirma, Esc/n cancela",
        Style::default().fg(theme::SUBTEXT),
    )));
    f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), inner);
}

pub fn draw_add_app(f: &mut Frame, area: Rect, form: &AddAppForm) {
    let area = layout::centered_rect(78, 78, area);
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::MANTLE)),
        area,
    );
    let block = Block::default()
        .title("Adicionar app no bench (apps.json) — produto ativo vinculante")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::TEAL));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let compact = inner.height < 16;
    let has_error = form.error.is_some();
    let dropdown_open =
        form.field == AppField::Ref && form.ref_mode == RefMode::Dropdown && form.ref_dropdown_open;
    let dropdown_h = if dropdown_open { 7 } else { 0 };

    let constraints = if compact {
        if has_error {
            vec![
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(dropdown_h),
                Constraint::Length(1),
                Constraint::Min(1),
            ]
        } else {
            vec![
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Length(dropdown_h),
                Constraint::Length(0),
                Constraint::Min(1),
            ]
        }
    } else if has_error {
        vec![
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(dropdown_h),
            Constraint::Length(2),
            Constraint::Min(2),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(dropdown_h),
            Constraint::Length(0),
            Constraint::Min(2),
        ]
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let intro_text = if compact {
        "Repo Git e ref vinculada ao produto ativo (existentes/aguardadas)."
    } else {
        "Informe o repositório Git e selecione uma ref existente/aguardada do produto ativo ou crie uma nova."
    };
    let intro = Paragraph::new(Line::from(Span::styled(
        intro_text,
        Style::default().fg(theme::SUBTEXT),
    )))
    .wrap(Wrap { trim: true });
    f.render_widget(intro, rows[0]);

    let url_border = if form.field == AppField::Url {
        theme::BLUE
    } else {
        theme::OVERLAY
    };
    let ref_border = if form.field == AppField::Ref {
        theme::BLUE
    } else {
        theme::OVERLAY
    };
    let url_value = if form.url.is_empty() {
        "https://github.com/org/repo".into()
    } else {
        form.url.clone()
    };
    let ref_value = if form.ref_value.is_empty() {
        match form.ref_mode {
            RefMode::Dropdown => "Selecione uma ref (↑/↓, Enter/Espaço)".into(),
            RefMode::CustomInput => "Digite nova ref (tag/commit)".into(),
        }
    } else {
        form.ref_value.clone()
    };
    let ref_title = match form.ref_mode {
        RefMode::Dropdown => "Ref (dropdown: existentes e aguardadas, produto ativo)",
        RefMode::CustomInput => "Ref (nova ref personalizada, produto ativo)",
    };

    let url_p = Paragraph::new(url_value)
        .style(Style::default().fg(if form.field == AppField::Url {
            theme::TEXT
        } else {
            theme::SUBTEXT
        }))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(url_border))
                .title("URL do repositório (Git)"),
        );
    let ref_p = Paragraph::new(ref_value)
        .style(Style::default().fg(if form.field == AppField::Ref {
            theme::TEXT
        } else {
            theme::SUBTEXT
        }))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ref_border))
                .title(ref_title),
        );
    f.render_widget(url_p, rows[1]);
    f.render_widget(ref_p, rows[2]);

    if dropdown_open {
        let items: Vec<ListItem> = if form.ref_options.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "Nenhuma ref encontrada — use Ctrl+n para criar nova",
                Style::default().fg(theme::SUBTEXT),
            )))]
        } else {
            form.ref_options
                .iter()
                .enumerate()
                .map(|(i, opt)| {
                    let selected = i == form.ref_selected;
                    let (prefix, color) = if opt.value.is_empty() {
                        ("＋ ", theme::GREEN)
                    } else if opt.awaited {
                        ("⏳ ", theme::YELLOW)
                    } else {
                        ("• ", theme::TEAL)
                    };
                    let style = if selected {
                        Style::default()
                            .fg(theme::TEXT)
                            .bg(theme::OVERLAY)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(opt.label.clone(), style),
                    ]))
                })
                .collect()
        };
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme::BLUE))
                .title("Refs do produto ativo (existentes/aguardadas)"),
        );
        f.render_widget(list, rows[3]);
    }

    if let Some(err) = &form.error {
        let err_p = Paragraph::new(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(theme::RED).add_modifier(Modifier::BOLD),
        )))
        .wrap(Wrap { trim: true });
        f.render_widget(err_p, rows[4]);
    }

    let hint_text = if compact {
        "Tab campo | Ref: ↑/↓ Espaço/Enter | Ctrl+n nova | Enter adiciona"
    } else {
        "Tab alterna campos | Ref: ↑/↓ seleciona, Espaço/Enter abre/confirma, Ctrl+n cria nova ref | Enter adiciona em apps.json | Esc cancela"
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(theme::SUBTEXT))
        .wrap(Wrap { trim: true });
    f.render_widget(hint, rows[5]);
}

pub fn draw_reset_env(f: &mut Frame, area: Rect, state: &AppState, form: &ResetEnvForm) {
    let area = layout::centered_rect(76, 54, area);
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::MANTLE)),
        area,
    );
    let block = Block::default()
        .title(format!("Reset .env — {}", form.product.label()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::RED));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let has_draft = state.new_product_drafts.contains_key(form.product.label());
    let mut modes: Vec<(ResetEnvMode, &str, &str)> = vec![
        (
            ResetEnvMode::RestoreExample,
            "Restaurar do .env.example",
            "Substitui o .env atual pelo exemplo (com backup).",
        ),
        (
            ResetEnvMode::RemoveEnv,
            "Remover o .env",
            "Remove o arquivo .env do produto (com backup).",
        ),
        (
            ResetEnvMode::RestoreBackup,
            "Restaurar backup",
            "Restaura o backup .env.bak mais recente.",
        ),
    ];
    if has_draft {
        modes.push((
            ResetEnvMode::RestoreNewProductDraft,
            "Restaurar valores do novo produto (sessão)",
            "Recria o .env com as variáveis informadas na criação (acessíveis mid-session).",
        ));
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "Ação destrutiva: afeta apenas o .env deste produto. Não altera volumes nem outras stacks.",
            Style::default().fg(theme::YELLOW),
        )),
        Line::from(""),
    ];
    for (mode, label, desc) in modes {
        let selected = mode == form.mode;
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                Style::default().fg(if selected {
                    theme::BLUE
                } else {
                    theme::SUBTEXT
                }),
            ),
            Span::styled(
                label,
                Style::default()
                    .fg(if selected {
                        theme::TEXT
                    } else {
                        theme::SUBTEXT
                    })
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(format!(" — {desc}"), Style::default().fg(theme::SUBTEXT)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "Para confirmar, digite o nome do produto: {}",
            form.product.label()
        ),
        Style::default().fg(theme::PEACH),
    )));
    lines.push(Line::from(vec![
        Span::styled("Confirmação: ", Style::default().fg(theme::SUBTEXT)),
        Span::styled(
            form.confirm_input.clone(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑/↓ seleciona modo | Enter executa | Esc cancela",
        Style::default().fg(theme::SUBTEXT),
    )));

    let p = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(p, inner);
}

pub fn draw_confirm_text(
    f: &mut Frame,
    area: Rect,
    title: &str,
    message: &str,
    required: &str,
    input: &str,
) {
    let area = layout::centered_rect(70, 35, area);
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::MANTLE)),
        area,
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::YELLOW));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(theme::TEXT),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Digite: {required}"),
            Style::default().fg(theme::PEACH),
        )),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme::BLUE)),
            Span::styled(
                input.to_string(),
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter confirma | Esc cancela",
            Style::default().fg(theme::SUBTEXT),
        )),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

pub fn draw_new_product(f: &mut Frame, area: Rect, form: &NewProductForm) {
    let area = layout::centered_rect(84, 82, area);
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::MANTLE)),
        area,
    );
    let block = Block::default()
        .title("Novo produto (automatizado)")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::TEAL));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(inner);

    let intro = Paragraph::new(vec![
        Line::from(Span::styled(
            "Informe as variáveis de antemão. Os valores ficam salvos na sessão para facilitar reuso/reset.",
            Style::default().fg(theme::SUBTEXT),
        )),
        Line::from(Span::styled(
            "O produto, .env, .env.example, apps.json e build.env serão criados automaticamente.",
            Style::default().fg(theme::SUBTEXT),
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(Block::default().borders(Borders::NONE));
    f.render_widget(intro, rows[0]);

    let fields = [
        (
            NewProductField::Id,
            "ID do produto",
            form.draft.id.as_str(),
            "Nome da pasta em products/<id> (obrigatório)",
        ),
        (
            NewProductField::ImageRef,
            "IMAGE_REF",
            form.draft.image_ref.as_str(),
            "Imagem/tag ou digest do produto",
        ),
        (
            NewProductField::DbPassword,
            "DB_PASSWORD",
            form.draft.db_password.as_str(),
            "Senha do banco (Ctrl+g gera)",
        ),
        (
            NewProductField::TunnelToken,
            "TUNNEL_TOKEN",
            form.draft.tunnel_token.as_str(),
            "Token do Cloudflare Tunnel",
        ),
        (
            NewProductField::EdgeSubnet,
            "EDGE_SUBNET",
            form.draft.edge_subnet.as_str(),
            "Rede de borda exclusiva",
        ),
        (
            NewProductField::EdgeIpRange,
            "EDGE_IP_RANGE",
            form.draft.edge_ip_range.as_str(),
            "Faixa de IPs dinâmicos",
        ),
        (
            NewProductField::CloudflaredIp,
            "CLOUDFLARED_IP",
            form.draft.cloudflared_ip.as_str(),
            "IP fixo do cloudflared",
        ),
        (
            NewProductField::FrappeRef,
            "FRAPPE_REF (build.env)",
            form.draft.frappe_ref.as_str(),
            "Tag/commit do Frappe Framework",
        ),
        (
            NewProductField::AppRepoUrl,
            "App repo URL (apps.json)",
            form.draft.app_repo_url.as_str(),
            "Repositório Git do app (opcional)",
        ),
        (
            NewProductField::AppRepoRef,
            "App repo ref (apps.json)",
            form.draft.app_repo_ref.as_str(),
            "Tag/commit/branch do app (opcional)",
        ),
    ];
    let items: Vec<ListItem> = fields
        .iter()
        .map(|(fie, label, value, hint)| {
            let active = *fie == form.field;
            let shown = if value.is_empty() { "<vazio>" } else { *value };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{label}: "),
                        Style::default()
                            .fg(if active { theme::BLUE } else { theme::TEAL })
                            .add_modifier(if active {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        shown.to_string(),
                        Style::default().fg(if active { theme::TEXT } else { theme::SUBTEXT }),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("  {hint}"),
                    Style::default().fg(theme::OVERLAY),
                )),
            ])
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::BLUE))
            .title("Variáveis iniciais (Tab navega, Enter cria)"),
    );
    f.render_widget(list, rows[1]);

    let mut footer_lines = vec![Line::from(Span::styled(
        "Tab/Shift+Tab: campos | Ctrl+g: gerar senha DB | Enter: criar produto | Esc: cancelar",
        Style::default().fg(theme::SUBTEXT),
    ))];
    if let Some(err) = &form.error {
        footer_lines.push(Line::from(Span::styled(
            format!("Erro: {err}"),
            Style::default().fg(theme::RED).add_modifier(Modifier::BOLD),
        )));
    } else {
        footer_lines.push(Line::from(Span::styled(
            "Dica: depois de criar, ajuste detalhes em Bridges & Plugins > Env e salve quando necessário.",
            Style::default().fg(theme::SUBTEXT),
        )));
    }
    f.render_widget(
        Paragraph::new(footer_lines)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Ação"),
            ),
        rows[2],
    );
}
