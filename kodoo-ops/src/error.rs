use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    /// Entrada invalida (nome de site, release, valor de .env...).
    Invalid(String),
    /// Arquivo ou raiz do deployment nao encontrado.
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "erro de E/S: {e}"),
            Error::Json(e) => write!(f, "JSON invalido: {e}"),
            Error::Invalid(m) => write!(f, "{m}"),
            Error::NotFound(m) => write!(f, "nao encontrado: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
