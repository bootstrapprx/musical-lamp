//! Geracao de segredos com o CSPRNG do sistema operacional (getrandom).
use crate::error::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    /// `n` bytes aleatorios em hexadecimal (2n caracteres). DB_PASSWORD padrao: Hex(24).
    Hex(usize),
    /// `n` caracteres [A-Za-z0-9] sem vies (amostragem por rejeicao).
    Alnum(usize),
    /// `n` bytes aleatorios em base64 URL-safe, sem padding.
    UrlSafe(usize),
}

pub fn generate(kind: SecretKind) -> Result<String> {
    match kind {
        SecretKind::Hex(n) => hex(n),
        SecretKind::Alnum(n) => alnum(n),
        SecretKind::UrlSafe(n) => url_safe(n),
    }
}

fn fill(n: usize) -> Result<Vec<u8>> {
    if n == 0 || n > 1024 {
        return Err(Error::Invalid(
            "tamanho do segredo deve estar entre 1 e 1024".into(),
        ));
    }
    let mut buf = vec![0u8; n];
    getrandom::getrandom(&mut buf)
        .map_err(|e| Error::Invalid(format!("CSPRNG indisponivel: {e}")))?;
    Ok(buf)
}

pub fn hex(nbytes: usize) -> Result<String> {
    Ok(fill(nbytes)?.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn alnum(len: usize) -> Result<String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    // 62 * 4 = 248: bytes >= 248 sao descartados para nao enviesar o modulo.
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

pub fn url_safe(nbytes: usize) -> Result<String> {
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

/// Tipo padrao de segredo para uma chave conhecida do .env.
pub fn default_kind_for(key: &str) -> Option<SecretKind> {
    match key {
        "DB_PASSWORD" => Some(SecretKind::Hex(24)),
        _ => None,
    }
}

/// Aviso operacional que a UI deve mostrar ANTES de trocar o valor desta chave.
pub fn rotation_note(key: &str) -> Option<&'static str> {
    match key {
        "DB_PASSWORD" => Some(
            "MYSQL_ROOT_PASSWORD so vale na criacao do volume db-data. Trocar aqui NAO altera a \
             senha do MariaDB ja inicializado: altere antes com ALTER USER (ou recrie o volume) \
             e so depois atualize o .env.",
        ),
        "TUNNEL_TOKEN" => Some(
            "Gere o novo token no painel Zero Trust (rotacione o tunnel) e depois reinicie o cloudflared.",
        ),
        _ => None,
    }
}

/// Chaves cujo valor deve ser mascarado na interface.
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
        "\u{2022}".repeat(value.chars().count().min(12))
    }
}
