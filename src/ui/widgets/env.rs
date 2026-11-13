use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::ui::theme;

pub fn draw(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(5)])
        .split(area);

    let Some(env) = state.current_env() else {
        let p = Paragraph::new("Sem estado de .env para o produto selecionado.")
            .block(Block::default().borders(Borders::ALL).title(".env"));
        f.render_widget(p, area);
        return;
    };

    let header_lines = vec![
        Line::from(vec![
            Span::styled("Produto: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(state.featured_product.label(), Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD)),
            Span::raw("  |  "),
            Span::styled("Arquivo: ", Style::default().fg(theme::SUBTEXT)),
            Span::styled(&env.path_hint, Style::default().fg(theme::TEXT)),
            Span::raw("  |  "),
            Span::styled(if env.dirty { "alterações pendentes" } else { "salvo/sincronizado" }, Style::default().fg(if env.dirty { theme::YELLOW } else { theme::GREEN })),
        ]),
        Line::from(vec![Span::styled("Ação: ", Style::default().fg(theme::SUBTEXT)), Span::styled(&env.last_action, Style::default().fg(theme::PEACH))]),
    ];
    let header = Paragraph::new(header_lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Ciclo de vida do .env"));
    f.render_widget(header, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(chunks[1]);

    let items: Vec<ListItem> = if env.vars.is_empty() {
        vec![ListItem::new(Line::from(Span::styled("Nenhuma variável encontrada. Use [g] para gerar a partir do .env.example.", Style::default().fg(theme::SUBTEXT))))]
    } else {
        env.vars.iter().enumerate().map(|(i, v)| {
            let selected = i == state.env_selected;
            let val_display = if v.value.is_empty() { "<vazio>" } else { v.value.as_str() };
            let line = Line::from(vec![
                Span::styled(format!("{:<24}", v.key), Style::default().fg(if selected { theme::BLUE } else { theme::TEAL }).add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() })),
                Span::raw(" = "),
                Span::styled(val_display.to_string(), Style::default().fg(if v.value.is_empty() { theme::YELLOW } else { theme::TEXT })),
            ]);
            ListItem::new(line)
        }).collect()
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BLUE))
        .title("Variáveis (↑/↓ selecionar, Enter editar)");
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(theme::OVERLAY))
        .highlight_symbol("› ");
    let mut st = ListState::default();
    if !env.vars.is_empty() { st.select(Some(state.env_selected.min(env.vars.len()-1))); }
    f.render_stateful_widget(list, body[0], &mut st);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(body[1]);

    let (key, value, comment) = if let Some(v) = env.vars.get(state.env_selected) {
        (v.key.clone(), if state.env_editing { state.env_input.clone() } else { v.value.clone() }, v.comment.clone().unwrap_or_else(|| "sem comentário".into()))
    } else {
        ("(nenhuma)".into(), String::new(), "Use [g] para gerar variáveis do exemplo".into())
    };
    let edit_title = if state.env_editing { "Edição de valor (ativo)" } else { "Detalhe da variável" };
    let detail = vec![
        Line::from(vec![Span::styled("Chave: ", Style::default().fg(theme::SUBTEXT)), Span::styled(key, Style::default().fg(theme::TEAL).add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("Valor: ", Style::default().fg(theme::SUBTEXT)), Span::styled(if value.is_empty() { "<vazio>".into() } else { value }, Style::default().fg(if state.env_editing { theme::GREEN } else { theme::TEXT }))]),
        Line::from(""),
        Line::from(vec![Span::styled("Comentário: ", Style::default().fg(theme::SUBTEXT)), Span::styled(comment, Style::default().fg(theme::SUBTEXT))]),
        Line::from(""),
        Line::from(Span::styled(if state.env_editing { "Editando: Enter confirma, Esc cancela, Backspace apaga" } else { "Enter para editar o valor selecionado" }, Style::default().fg(if state.env_editing { theme::YELLOW } else { theme::OVERLAY }))),
    ];
    let detail_p = Paragraph::new(detail)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(if state.env_editing { theme::GREEN } else { theme::OVERLAY })).title(edit_title));
    f.render_widget(detail_p, right[0]);

    let help = vec![
        Line::from(Span::styled("Ciclo de vida suportado", Style::default().fg(theme::MAUVE).add_modifier(Modifier::BOLD))),
        Line::from("• [g] Gerar: carrega variáveis de products/<produto>/.env.example"),
        Line::from("• [Enter] Editar: altera valor em memória"),
        Line::from("• [w] Salvar: grava products/<produto>/.env"),
        Line::from("• [r] Recarregar: lê .env (ou exemplo) do disco"),
        Line::from("• [p] Alterna produto em destaque (accounting/trivia)"),
        Line::from(""),
        Line::from(Span::styled("Nota: salvamento é local no workspace; segredos devem ser revisados antes do deploy.", Style::default().fg(theme::YELLOW))),
    ];
    let help_p = Paragraph::new(help)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title("Manutenção"));
    f.render_widget(help_p, right[1]);
}
