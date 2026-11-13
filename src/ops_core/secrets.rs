#![allow(dead_code)]
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    Hex(usize),
    Alnum(usize),
    UrlSafe(usize),
}

impl fmt::Display for SecretKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretKind::Hex(n) => write!(f, "hex({n})"),
            SecretKind::Alnum(n) => write!(f, "alnum({n})"),
            SecretKind::UrlSafe(n) => write!(f, "urlsafe({n})"),
        }
    }
}

pub fn generate(kind: SecretKind) -> Result<String, String> {
    match kind {
        SecretKind::Hex(n) => hex(n),
        SecretKind::Alnum(n) => alnum(n),
        SecretKind::UrlSafe(n) => url_safe(n),
    }
}

fn fill(n: usize) -> Result<Vec<u8>, String> {
    if n == 0 || n > 1024 {
        return Err("tamanho inválido".into());
    }
    let mut buf = vec![0u8; n];
    getrandom::getrandom(&mut buf).map_err(|e| format!("CSPRNG indisponível: {e}"))?;
    Ok(buf)
}

pub fn hex(nbytes: usize) -> Result<String, String> {
    Ok(fill(nbytes)?.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn alnum(len: usize) -> Result<String, String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        for b in fill(len.max(16))? {
            if b < 248 {
                out.push(CHARS[(b % 62) as usize] as char);
                if out.len() == len {
                    break;
                }
            }
        }
    }
    Ok(out)
}

pub fn url_safe(nbytes: usize) -> Result<String, String> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let data = fill(nbytes)?;
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    Ok(out)
}

pub fn default_kind_for(key: &str) -> Option<SecretKind> {
    let k = key.to_ascii_uppercase();
    if k == "DB_PASSWORD" || k.ends_with("_PASSWORD") || k.contains("PASSWORD") {
        return Some(SecretKind::Hex(24));
    }
    if k == "TUNNEL_TOKEN" || k.ends_with("_TOKEN") || k.contains("TOKEN") {
        // Tokens are often externally issued; generate a strong placeholder/value when user asks.
        return Some(SecretKind::UrlSafe(32));
    }
    if k.ends_with("_SECRET") || k.contains("SECRET") {
        return Some(SecretKind::Hex(32));
    }
    if k.ends_with("_KEY") || k.contains("API_KEY") || k.contains("PRIVATE_KEY") {
        return Some(SecretKind::UrlSafe(32));
    }
    None
}

pub fn can_auto_generate(key: &str) -> bool {
    default_kind_for(key).is_some()
}

pub fn rotation_note(key: &str) -> Option<&'static str> {
    let k = key.to_ascii_uppercase();
    if k == "DB_PASSWORD" {
        return Some("Trocar DB_PASSWORD no .env não altera a senha do MariaDB já inicializado; ajuste no banco/volume conforme procedimento.");
    }
    if k == "TUNNEL_TOKEN" || k.contains("TUNNEL") {
        return Some("Se for token do Cloudflare, copie o valor emitido no painel Zero Trust; valores gerados aqui servem como placeholder/local.");
    }
    if k.contains("TOKEN") || k.contains("SECRET") || k.contains("KEY") || k.contains("PASSWORD") {
        return Some("Revise onde este segredo é usado antes de rotacionar em produção.");
    }
    None
}

pub fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    ["PASSWORD", "TOKEN", "SECRET", "KEY"]
        .iter()
        .any(|s| k.contains(s))
}

pub fn mask(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "•".repeat(value.chars().count().min(12))
    }
}
