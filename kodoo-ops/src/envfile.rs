//! Leitor/escritor de .env que preserva ordem, comentarios e linhas em branco.
use crate::error::{Error, Result};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Blank,
    Comment(String),
    /// `raw` e a linha completa como sera escrita (inalterada se ninguem chamou `set`).
    Var {
        key: String,
        value: String,
        trailing: String,
        raw: String,
    },
    Other(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnvFile {
    lines: Vec<Line>,
}

fn valid_key(k: &str) -> bool {
    let mut c = k.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Separa o valor do comentario final. Retorna (valor, trailing).
fn split_value(rest: &str) -> (String, String) {
    let r = rest.trim_start();
    if r.starts_with('#') {
        return (String::new(), rest.to_string());
    }
    if let Some(q) = r.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let body = &r[1..];
        let mut esc = false;
        for (i, ch) in body.char_indices() {
            if q == '"' && esc {
                esc = false;
                continue;
            }
            if q == '"' && ch == '\\' {
                esc = true;
                continue;
            }
            if ch == q {
                return (body[..i].to_string(), body[i + 1..].to_string());
            }
        }
    }
    let b = rest.as_bytes();
    for i in 1..b.len() {
        if b[i] == b'#' && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
            let before = &rest[..i];
            let end = before.trim_end().len();
            return (
                rest[..end].trim_start().to_string(),
                rest[end..].to_string(),
            );
        }
    }
    (rest.trim().to_string(), String::new())
}

fn parse_line(l: &str) -> Line {
    let t = l.trim();
    if t.is_empty() {
        return Line::Blank;
    }
    if t.starts_with('#') {
        return Line::Comment(l.to_string());
    }
    if let Some(eq) = l.find('=') {
        let key = l[..eq].trim();
        if valid_key(key) {
            let (value, trailing) = split_value(&l[eq + 1..]);
            return Line::Var {
                key: key.to_string(),
                value,
                trailing,
                raw: l.to_string(),
            };
        }
    }
    Line::Other(l.to_string())
}

/// Formata um valor para o .env (aspas simples quando necessario).
pub fn format_value(v: &str) -> Result<String> {
    if v.contains('\n') || v.contains('\r') {
        return Err(Error::Invalid(
            "valor de .env nao pode ter quebra de linha".into(),
        ));
    }
    let special = |c: char| c.is_whitespace() || matches!(c, '#' | '$' | '"' | '\'' | '\\' | '`');
    if !v.chars().any(special) {
        return Ok(v.to_string());
    }
    if !v.contains('\'') {
        return Ok(format!("'{v}'"));
    }
    if !v.chars().any(|c| matches!(c, '"' | '$' | '\\' | '`')) {
        return Ok(format!("\"{v}\""));
    }
    Err(Error::Invalid(
        "valor mistura aspas simples com caracteres especiais; simplifique".into(),
    ))
}

impl EnvFile {
    pub fn parse(text: &str) -> EnvFile {
        EnvFile {
            lines: text.lines().map(parse_line).collect(),
        }
    }

    pub fn load(path: &Path) -> Result<EnvFile> {
        match fs::read_to_string(path) {
            Ok(t) => Ok(EnvFile::parse(&t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(Error::NotFound(path.display().to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for l in &self.lines {
            match l {
                Line::Blank => {}
                Line::Comment(s) | Line::Other(s) => out.push_str(s),
                Line::Var { raw, .. } => out.push_str(raw),
            }
            out.push('\n');
        }
        out
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Valor da ultima ocorrencia (mesma regra do dotenv/compose).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|l| match l {
            Line::Var { key: k, value, .. } if k == key => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn is_blank(&self, key: &str) -> bool {
        self.get(key).map_or(true, |v| v.trim().is_empty())
    }

    pub fn keys(&self) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for l in &self.lines {
            if let Line::Var { key, .. } = l {
                if !seen.contains(&key.as_str()) {
                    seen.push(key);
                }
            }
        }
        seen
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        if !valid_key(key) {
            return Err(Error::Invalid(format!("nome de variavel invalido: {key}")));
        }
        let formatted = format_value(value)?;
        let slot = self
            .lines
            .iter_mut()
            .rev()
            .find(|l| matches!(l, Line::Var { key: k, .. } if k == key));
        match slot {
            Some(Line::Var {
                value: v,
                trailing,
                raw,
                ..
            }) => {
                *v = value.to_string();
                *raw = format!("{key}={formatted}{trailing}");
            }
            _ => self.lines.push(Line::Var {
                key: key.to_string(),
                value: value.to_string(),
                trailing: String::new(),
                raw: format!("{key}={formatted}"),
            }),
        }
        Ok(())
    }

    /// Grava de forma atomica com permissao 0600, guardando backup (.bak-<unix>, ultimos 5).
    pub fn save(&self, path: &Path) -> Result<()> {
        if path.exists() {
            backup(path)?;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(".env");
        let tmp = path.with_file_name(format!("{name}.kodoo-tmp"));
        write_private(&tmp, self.to_text().as_bytes())?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(unix)]
fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    fs::write(path, data)?;
    Ok(())
}

fn backup(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".env")
        .to_string();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let data = fs::read(path)?;
    write_private(&dir.join(format!("{name}.bak-{ts}")), &data)?;
    let prefix = format!("{name}.bak-");
    let mut baks: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(&prefix))
        .collect();
    baks.sort();
    while baks.len() > 5 {
        let old = baks.remove(0);
        let _ = fs::remove_file(dir.join(old));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# cabecalho\n\nIMAGE_REF=kodoo/trivia:RELEASE\nCUSTOM=latest  # troque\nQUOTED=\"a b\"\nEMPTY=\n";

    #[test]
    fn roundtrip_is_exact() {
        assert_eq!(EnvFile::parse(SAMPLE).to_text(), SAMPLE);
    }

    #[test]
    fn get_and_trailing_comment() {
        let e = EnvFile::parse(SAMPLE);
        assert_eq!(e.get("CUSTOM"), Some("latest"));
        assert_eq!(e.get("QUOTED"), Some("a b"));
        assert!(e.is_blank("EMPTY"));
        assert_eq!(e.get("NOPE"), None);
    }

    #[test]
    fn set_keeps_comment_and_appends_new() {
        let mut e = EnvFile::parse(SAMPLE);
        e.set("CUSTOM", "v2").unwrap();
        e.set("NOVA", "x y").unwrap();
        let t = e.to_text();
        assert!(t.contains("CUSTOM=v2  # troque\n"));
        assert!(t.ends_with("NOVA='x y'\n"));
    }

    #[test]
    fn rejects_newline_and_bad_key() {
        let mut e = EnvFile::default();
        assert!(e.set("A", "l1\nl2").is_err());
        assert!(e.set("1A", "x").is_err());
    }
}
