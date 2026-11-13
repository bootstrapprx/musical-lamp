use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{AddAppForm, AddField, AddNodeForm, AppField, AppState};
use crate::ui::{layout, theme};

pub fn draw_add_node(f: &mut Frame, area: Rect, form: &AddNodeForm) {
    let area = layout::centered_rect(70, 80, area);
    f.render_widget(Clear, area);
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

    draw_field(f, rows[0], "Nome do Nó", &form.name, form.field == AddField::Name);
    draw_field(f, rows[1], "IP/Host", &form.host, form.field == AddField::Host);
    draw_field(f, rows[2], "Usuário SSH", &form.user, form.field == AddField::User);
    draw_field(f, rows[3], "Provedor", &form.provider, form.field == AddField::Provider);
    draw_field(f, rows[4], "Custo Mensal ($)", &form.cost, form.field == AddField::Cost);
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
    let title = if active { format!("{label} (ativo)") } else { label.to_string() };
    if area.height <= 1 {
        let line = Line::from(vec![
            Span::styled(format!("{label}: "), Style::default().fg(border).add_modifier(Modifier::BOLD)),
            Span::styled(value.to_string(), Style::default().fg(if active { theme::TEXT } else { theme::SUBTEXT })),
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
    let area = layout::centered_rect(65, 70, area);
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
        Line::from(Span::styled(
            "Confirmar remoção segura do nó selecionado?",
            Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("Nó: {name}  |  Custo fixo: ${cost:.2}/mo")),
        Line::from(format!("Containers que precisarão realocação: {count}")),
        Line::from(format!(
            "Impacto: orçamento projetado ${:.2} -> ${:.2}/mo",
            state.projected_total(),
            after
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if confirm { "[ Confirmar ]" } else { "  Confirmar  " },
                Style::default()
                    .fg(if confirm { theme::BASE } else { theme::GREEN })
                    .bg(if confirm { theme::GREEN } else { theme::MANTLE }),
            ),
            Span::raw("   "),
            Span::styled(
                if !confirm { "[ Cancelar ]" } else { "  Cancelar  " },
                Style::default()
                    .fg(if !confirm { theme::BASE } else { theme::SUBTEXT })
                    .bg(if !confirm { theme::YELLOW } else { theme::MANTLE }),
            ),
        ]),
        Line::from(Span::styled(
            "Tab/←/→ alterna, Enter confirma, Esc/N cancela",
            Style::default().fg(theme::SUBTEXT),
        )),
    ];
    f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), inner);
}

pub fn draw_add_app(f: &mut Frame, area: Rect, form: &AddAppForm) {
    let area = layout::centered_rect(75, 70, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .title("Adicionar app no bench (apps.json)")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::TEAL));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let compact = inner.height < 10;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if compact {
            [Constraint::Length(1), Constraint::Length(2), Constraint::Length(2), Constraint::Min(1)]
        } else {
            [Constraint::Length(2), Constraint::Length(3), Constraint::Length(3), Constraint::Min(2)]
        })
        .split(inner);

    let intro_text = if compact {
        "Repo Git e referência fixa (tag/commit)."
    } else {
        "Informe o repositório Git do app e a tag/commit (referência fixa) para o bench instalar."
    };
    let intro = Paragraph::new(Line::from(Span::styled(
        intro_text,
        Style::default().fg(theme::SUBTEXT),
    ))).wrap(Wrap { trim: true });
    f.render_widget(intro, rows[0]);

    let url_border = if form.field == AppField::Url { theme::BLUE } else { theme::OVERLAY };
    let branch_border = if form.field == AppField::Branch { theme::BLUE } else { theme::OVERLAY };
    let url_value = if form.url.is_empty() { "https://github.com/org/repo".into() } else { form.url.clone() };
    let branch_value = if form.branch.is_empty() { "v1.2.3 ou commit SHA".into() } else { form.branch.clone() };
    let url_p = Paragraph::new(url_value)
        .style(Style::default().fg(if form.field == AppField::Url { theme::TEXT } else { theme::SUBTEXT }))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(url_border)).title("URL do repositório (Git)"));
    let branch_p = Paragraph::new(branch_value)
        .style(Style::default().fg(if form.field == AppField::Branch { theme::TEXT } else { theme::SUBTEXT }))
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(branch_border)).title("Branch/Tag/Commit (referência)"));
    f.render_widget(url_p, rows[1]);
    f.render_widget(branch_p, rows[2]);

    let hint_text = if compact {
        "Tab alterna | Enter adiciona | Esc cancela"
    } else {
        "Tab alterna | Enter adiciona em apps.json | Esc cancela"
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(theme::SUBTEXT))
        .wrap(Wrap { trim: true });
    f.render_widget(hint, rows[3]);
}
