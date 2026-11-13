use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Write},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertLevel {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Border {
    Rounded,
    Single,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Cyan,
    Green,
    Red,
    Yellow,
    Blue,
    White,
    Gray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trend {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Outline,
}

#[derive(Clone, Debug)]
pub struct Theme {
    #[allow(dead_code)]
    dark: bool,
}

impl Theme {
    pub fn dark() -> Self {
        Self { dark: true }
    }
    pub fn light() -> Self {
        Self { dark: false }
    }
}

#[derive(Clone, Debug)]
pub struct RunConfig {
    theme: Theme,
    mouse: bool,
    tick_rate: Duration,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            theme: Theme::dark(),
            mouse: false,
            tick_rate: Duration::from_millis(16),
        }
    }
}

impl RunConfig {
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }
    pub fn mouse(mut self, mouse: bool) -> Self {
        self.mouse = mouse;
        self
    }
    pub fn tick_rate(mut self, rate: Duration) -> Self {
        self.tick_rate = rate;
        self
    }
}

#[derive(Clone, Debug)]
pub struct TableState {
    headers: Vec<String>,
    #[allow(dead_code)]
    rows: Vec<Vec<String>>,
    #[allow(dead_code)]
    selected: usize,
}

impl TableState {
    pub fn new<S: Into<String>, R: IntoIterator<Item = S>>(
        headers: impl IntoIterator<Item = S>,
        rows: impl IntoIterator<Item = R>,
    ) -> Self {
        Self {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(Into::into).collect())
                .collect(),
            selected: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ListState {
    items: Vec<String>,
    selected: usize,
}

impl ListState {
    pub fn new<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
}

#[derive(Clone, Debug)]
pub struct TextInputState {
    placeholder: String,
    #[allow(dead_code)]
    value: String,
}

impl TextInputState {
    pub fn with_placeholder(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            value: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SelectState {
    items: Vec<String>,
    selected: usize,
}

impl SelectState {
    pub fn new<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }
}

pub struct ButtonResult {
    pub clicked: bool,
}

pub struct Context {
    should_quit: bool,
    last_key: Option<KeyCode>,
    output: String,
}

impl Context {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            last_key: None,
            output: String::new(),
        }
    }

    pub fn key(&self, c: char) -> bool {
        match self.last_key {
            Some(KeyCode::Char(ch)) => ch == c,
            _ => false,
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    // Builder methods returning &mut Self or self for chaining
    pub fn container(&mut self) -> &mut Self {
        self
    }
    pub fn row<F: FnOnce(&mut Context)>(&mut self, f: F) -> &mut Self {
        f(self);
        self
    }
    pub fn col<F: FnOnce(&mut Context)>(&mut self, f: F) -> &mut Self {
        f(self);
        self
    }
    pub fn bordered(&mut self, _border: Border) -> &mut Self {
        self
    }
    pub fn px(&mut self, _n: usize) -> &mut Self {
        self
    }
    pub fn pad(&mut self, _n: usize) -> &mut Self {
        self
    }
    pub fn gap(&mut self, _n: usize) -> &mut Self {
        self
    }
    pub fn grow(&mut self, _n: usize) -> &mut Self {
        self
    }
    pub fn w(&mut self, _n: usize) -> &mut Self {
        self
    }
    pub fn spacer(&mut self) -> &mut Self {
        self
    }
    pub fn separator(&mut self) -> &mut Self {
        self
    }

    pub fn text(&mut self, s: &str) -> &mut Self {
        self.output.push_str(s);
        self
    }
    pub fn bold(&mut self) -> &mut Self {
        self
    }
    pub fn dim(&mut self) -> &mut Self {
        self
    }
    pub fn fg(&mut self, _color: Color) -> &mut Self {
        self
    }

    pub fn badge(&mut self, s: &str) -> &mut Self {
        self.output.push_str(&format!(" [{}] ", s));
        self
    }

    pub fn stat(&mut self, label: &str, value: &str) -> &mut Self {
        self.output.push_str(&format!("{}: {}  ", label, value));
        self
    }

    pub fn stat_trend(&mut self, label: &str, value: &str, trend: Trend) -> &mut Self {
        let arrow = match trend {
            Trend::Up => "▲",
            Trend::Down => "▼",
        };
        self.output
            .push_str(&format!("{} {} {}  ", label, value, arrow));
        self
    }

    pub fn table(&mut self, state: &mut TableState) -> io::Result<()> {
        self.output
            .push_str(&format!("Table: {} columns\n", state.headers.len()));
        Ok(())
    }

    pub fn alert(&mut self, msg: &str, level: AlertLevel) -> io::Result<()> {
        let prefix = match level {
            AlertLevel::Error => "ERROR",
            AlertLevel::Warning => "WARN",
            AlertLevel::Info => "INFO",
        };
        self.output.push_str(&format!("[{}] {}\n", prefix, msg));
        Ok(())
    }

    pub fn help(&mut self, items: &[(&str, &str)]) -> io::Result<()> {
        let help_str: Vec<String> = items.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
        self.output
            .push_str(&format!("Help: {}\n", help_str.join(" | ")));
        Ok(())
    }

    pub fn list(&mut self, state: &mut ListState) -> io::Result<()> {
        for (i, item) in state.items.iter().enumerate() {
            let marker = if i == state.selected { ">" } else { " " };
            self.output.push_str(&format!("{} {}\n", marker, item));
        }
        Ok(())
    }

    pub fn definition_list(&mut self, items: &[(&str, &str)]) -> &mut Self {
        for (k, v) in items {
            self.output.push_str(&format!("{}: {}\n", k, v));
        }
        self
    }

    pub fn text_input(&mut self, state: &mut TextInputState) -> io::Result<()> {
        self.output
            .push_str(&format!("[Input: {}]\n", state.placeholder));
        Ok(())
    }

    pub fn select(&mut self, state: &mut SelectState) -> io::Result<()> {
        for (i, item) in state.items.iter().enumerate() {
            let marker = if i == state.selected { "*" } else { " " };
            self.output.push_str(&format!("  {} {}\n", marker, item));
        }
        Ok(())
    }

    pub fn checkbox(&mut self, label: &str, checked: &mut bool) -> &mut Self {
        let box_char = if *checked { "[x]" } else { "[ ]" };
        self.output.push_str(&format!("{} {}\n", box_char, label));
        self
    }

    pub fn button(&mut self, label: &str) -> ButtonResult {
        self.output.push_str(&format!("[Button: {}]\n", label));
        ButtonResult { clicked: false }
    }

    pub fn button_with(&mut self, label: &str, _variant: ButtonVariant) -> ButtonResult {
        self.button(label)
    }

    pub fn bar_chart(&mut self, data: &[(&str, f64)], _max: usize) -> &mut Self {
        for (k, v) in data {
            self.output.push_str(&format!("{}: {}\n", k, v));
        }
        self
    }

    pub fn sparkline(&mut self, data: &[u64]) -> &mut Self {
        self.output.push_str(&format!("Sparkline: {:?}\n", data));
        self
    }
}

pub fn run_with<F>(config: RunConfig, mut callback: F) -> io::Result<()>
where
    F: FnMut(&mut Context),
{
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let mut ctx = Context::new();

    loop {
        ctx.output.clear();
        callback(&mut ctx);

        // Render to screen
        execute!(stdout, crossterm::cursor::MoveTo(0, 0))?;
        print!("{}", ctx.output);
        stdout.flush()?;

        if ctx.should_quit {
            break;
        }

        if event::poll(config.tick_rate)? {
            if let Event::Key(key) = event::read()? {
                ctx.last_key = Some(key.code);
                if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::NONE) {
                    // let callback handle quit or let us check
                }
            }
        } else {
            ctx.last_key = None;
        }
    }

    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
