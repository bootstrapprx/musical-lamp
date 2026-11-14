//! .env reader/writer preserving order, comments and blank lines (adapted from kodoo-ops).
#![allow(dead_code)]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn resolve_symlink_chain(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    for _ in 0..40 {
        let meta = match fs::symlink_metadata(&current) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(current),
            Err(e) => return Err(e.to_string()),
        };
        if !meta.file_type().is_symlink() {
            return Ok(current);
        }
        let link = fs::read_link(&current).map_err(|e| e.to_string())?;
        current = if link.is_absolute() {
            link
        } else {
            current
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(link)
        };
    }
    Err("cadeia de links simbólicos muito profunda ou em ciclo".into())
}

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Blank,
    Comment(String),
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

pub fn format_value(v: &str) -> Result<String, String> {
    if v.contains('\n') || v.contains('\r') {
        return Err("valor de .env não pode ter quebra de linha".into());
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
    Err("valor mistura aspas simples com caracteres especiais".into())
}

impl EnvFile {
    pub fn parse(text: &str) -> EnvFile {
        EnvFile {
            lines: text.lines().map(parse_line).collect(),
        }
    }

    pub fn load(path: &Path) -> Result<EnvFile, String> {
        match fs::read_to_string(path) {
            Ok(t) => Ok(EnvFile::parse(&t)),
            Err(e) => Err(format!("{}: {e}", path.display())),
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

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|l| match l {
            Line::Var { key: k, value, .. } if k == key => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn is_blank(&self, key: &str) -> bool {
        self.get(key).is_none_or(|v| v.trim().is_empty())
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

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !valid_key(key) {
            return Err(format!("nome de variável inválido: {key}"));
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

    pub fn save(&self, path: &Path) -> Result<(), String> {
        // Se for link simbólico (inclusive encadeado), gravar no destino final para não quebrar os links.
        let target = resolve_symlink_chain(path)?;

        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        if target.exists() {
            let _ = backup(&target);
        }
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".env");
        let tmp = target.with_file_name(format!("{name}.kodoo-tmp"));
        write_private(&tmp, self.to_text().as_bytes()).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(unix)]
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
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
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::write(path, data)
}

fn backup(path: &Path) -> Result<(), String> {
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
    let data = fs::read(path).map_err(|e| e.to_string())?;
    write_private(&dir.join(format!("{name}.bak-{ts}")), &data).map_err(|e| e.to_string())?;
    Ok(())
}
