pub use crossterm::event::KeyEvent;

use crate::domain::Product;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Resize,
    Tick,
    OpsLine {
        product: Product,
        line: String,
    },
    OpsFinished {
        product: Product,
        action: String,
        success: bool,
        summary: String,
    },
}

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    RunOps {
        product: Product,
        action: String,
        cmd: String,
        args: Vec<String>,
    },
}
