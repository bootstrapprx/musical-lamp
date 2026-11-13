use crate::ops_core::envfile::EnvFile;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub key: Option<String>,
    pub message: String,
}

impl Finding {
    fn new(severity: Severity, key: Option<&str>, message: impl Into<String>) -> Finding {
        Finding {
            severity,
            key: key.map(String::from),
            message: message.into(),
        }
    }
}

fn parse_ipv4(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut v = 0u32;
    for p in parts {
        let b: u8 = p.parse().ok()?;
        v = (v << 8) | b as u32;
    }
    Some(v)
}
fn parse_cidr(s: &str) -> Option<(u32, u8)> {
    let (a, b) = s.trim().split_once('/')?;
    let ip = parse_ipv4(a)?;
    let p: u8 = b.parse().ok()?;
    (p <= 32).then_some((ip, p))
}
fn mask(p: u8) -> u32 {
    if p == 0 {
        0
    } else {
        u32::MAX << (32 - p)
    }
}
fn network(ip: u32, p: u8) -> u32 {
    ip & mask(p)
}
fn contains(c: (u32, u8), ip: u32) -> bool {
    network(ip, c.1) == network(c.0, c.1)
}
fn overlaps(a: (u32, u8), b: (u32, u8)) -> bool {
    let p = a.1.min(b.1);
    network(a.0, p) == network(b.0, p)
}
fn broadcast(c: (u32, u8)) -> u32 {
    network(c.0, c.1) | !mask(c.1)
}

const REQUIRED: &[&str] = &[
    "IMAGE_REF",
    "DB_PASSWORD",
    "TUNNEL_TOKEN",
    "CLOUDFLARED_VERSION",
    "EDGE_SUBNET",
    "EDGE_IP_RANGE",
    "CLOUDFLARED_IP",
];

pub fn validate_env(env: &EnvFile, env_path: &Path, other: Option<&EnvFile>) -> Vec<Finding> {
    use Severity::*;
    let mut f = Vec::new();
    for k in REQUIRED {
        if env.is_blank(k) {
            f.push(Finding::new(Error, Some(k), format!("{k} está vazio")));
        }
    }
    if let Some(v) = env.get("IMAGE_REF").filter(|v| !v.is_empty()) {
        if v.contains("RELEASE") {
            f.push(Finding::new(
                Error,
                Some("IMAGE_REF"),
                "ainda é placeholder RELEASE",
            ));
        } else if let Some((_, d)) = v.split_once("@sha256:") {
            if d.len() != 64 || !d.chars().all(|c| c.is_ascii_hexdigit()) {
                f.push(Finding::new(
                    Error,
                    Some("IMAGE_REF"),
                    "digest sha256 malformado",
                ));
            }
        } else {
            let tag = v
                .rsplit_once(':')
                .filter(|(_, t)| !t.contains('/'))
                .map(|(_, t)| t);
            match tag {
                None | Some("latest") => f.push(Finding::new(
                    Error,
                    Some("IMAGE_REF"),
                    "sem tag fixa (latest)",
                )),
                Some(_) => f.push(Finding::new(
                    Warn,
                    Some("IMAGE_REF"),
                    "em produção prefira digest @sha256:...",
                )),
            }
        }
    }
    if let Some(v) = env.get("DB_PASSWORD").filter(|v| !v.is_empty()) {
        if v.chars().count() < 24 {
            f.push(Finding::new(
                Warn,
                Some("DB_PASSWORD"),
                "menos de 24 caracteres",
            ));
        }
    }
    if let Some(v) = env.get("CLOUDFLARED_VERSION").filter(|v| !v.is_empty()) {
        if v == "latest" {
            f.push(Finding::new(
                Error,
                Some("CLOUDFLARED_VERSION"),
                "fixe uma tag; latest não é reprodutível",
            ));
        }
    }
    if let Some(v) = env.get("TUNNEL_TOKEN").filter(|v| !v.is_empty()) {
        let ok = v.len() >= 50
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'));
        if !ok {
            f.push(Finding::new(
                Warn,
                Some("TUNNEL_TOKEN"),
                "formato incomum para token de túnel",
            ));
        }
    }
    let subnet = env.get("EDGE_SUBNET").and_then(parse_cidr);
    let range = env.get("EDGE_IP_RANGE").and_then(parse_cidr);
    let cf_ip = env.get("CLOUDFLARED_IP").and_then(parse_ipv4);
    if !env.is_blank("EDGE_SUBNET") && subnet.is_none() {
        f.push(Finding::new(
            Error,
            Some("EDGE_SUBNET"),
            "CIDR IPv4 inválido",
        ));
    }
    if !env.is_blank("EDGE_IP_RANGE") && range.is_none() {
        f.push(Finding::new(
            Error,
            Some("EDGE_IP_RANGE"),
            "CIDR IPv4 inválido",
        ));
    }
    if !env.is_blank("CLOUDFLARED_IP") && cf_ip.is_none() {
        f.push(Finding::new(Error, Some("CLOUDFLARED_IP"), "IPv4 inválido"));
    }
    if let Some(s) = subnet {
        if network(s.0, s.1) != s.0 {
            f.push(Finding::new(
                Warn,
                Some("EDGE_SUBNET"),
                "bits de host preenchidos; use o endereço de rede",
            ));
        }
        if let Some(r) = range {
            if !(contains(s, network(r.0, r.1)) && contains(s, broadcast(r))) {
                f.push(Finding::new(
                    Error,
                    Some("EDGE_IP_RANGE"),
                    "fora de EDGE_SUBNET",
                ));
            }
        }
        if let Some(ip) = cf_ip {
            let net = network(s.0, s.1);
            if !contains(s, ip) {
                f.push(Finding::new(
                    Error,
                    Some("CLOUDFLARED_IP"),
                    "fora de EDGE_SUBNET",
                ));
            } else if ip == net || ip == broadcast(s) {
                f.push(Finding::new(
                    Error,
                    Some("CLOUDFLARED_IP"),
                    "endereço de rede/broadcast",
                ));
            } else if ip == net + 1 {
                f.push(Finding::new(
                    Error,
                    Some("CLOUDFLARED_IP"),
                    "é o gateway do Docker (primeiro IP útil)",
                ));
            }
            if let Some(r) = range {
                if contains(r, ip) {
                    f.push(Finding::new(
                        Error,
                        Some("CLOUDFLARED_IP"),
                        "dentro de EDGE_IP_RANGE: conflito com frontend",
                    ));
                }
            }
        }
        if let Some(o) = other
            .and_then(|o| o.get("EDGE_SUBNET"))
            .and_then(parse_cidr)
        {
            if overlaps(s, o) {
                f.push(Finding::new(
                    Error,
                    Some("EDGE_SUBNET"),
                    "colide com a sub-rede do outro produto",
                ));
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = fs::metadata(env_path) {
            if m.permissions().mode() & 0o077 != 0 {
                f.push(Finding::new(
                    Warn,
                    None,
                    ".env legível por grupo/outros; recomendado chmod 600",
                ));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = env_path;
    f
}

pub fn drift(env: &EnvFile, example: &EnvFile) -> Vec<Finding> {
    let mut f = Vec::new();
    for k in example.keys() {
        if env.get(k).is_none() {
            f.push(Finding::new(
                Severity::Warn,
                Some(k),
                "existe no .env.example e falta no .env",
            ));
        }
    }
    for k in env.keys() {
        if example.get(k).is_none() {
            f.push(Finding::new(
                Severity::Info,
                Some(k),
                "não existe no .env.example (obsoleta/local)",
            ));
        }
    }
    f
}

pub fn validate_build(product_dir: &Path) -> Vec<Finding> {
    use Severity::*;
    let mut f = Vec::new();
    let build_env = product_dir.join("build.env");
    let apps_json = product_dir.join("apps.json");
    match EnvFile::load(&build_env) {
        Ok(b) => {
            match b.get("FRAPPE_REF").filter(|v| !v.is_empty()) {
                None => f.push(Finding::new(
                    Error,
                    Some("FRAPPE_REF"),
                    "vazio em build.env",
                )),
                Some(v)
                    if v.starts_with("version-") || matches!(v, "develop" | "main" | "master") =>
                {
                    f.push(Finding::new(
                        Error,
                        Some("FRAPPE_REF"),
                        "branch móvel; use tag ou commit",
                    ))
                }
                Some(_) => {}
            }
            if b.is_blank("IMAGE") {
                f.push(Finding::new(Error, Some("IMAGE"), "vazio em build.env"));
            }
        }
        Err(e) => f.push(Finding::new(Info, None, format!("build.env: {e}"))),
    }
    match fs::read_to_string(&apps_json) {
        Err(e) => f.push(Finding::new(Info, None, format!("apps.json: {e}"))),
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Err(e) => f.push(Finding::new(
                Error,
                None,
                format!("apps.json inválido: {e}"),
            )),
            Ok(v) => match v.as_array() {
                None => f.push(Finding::new(Error, None, "apps.json deve ser uma lista")),
                Some(apps) => {
                    for (i, a) in apps.iter().enumerate() {
                        for field in ["url", "branch"] {
                            match a.get(field).and_then(|x| x.as_str()) {
                                None => f.push(Finding::new(
                                    Error,
                                    None,
                                    format!("apps.json[{i}]: falta {field}"),
                                )),
                                Some(s) if s.contains('<') || s.contains('>') => {
                                    f.push(Finding::new(
                                        Error,
                                        None,
                                        format!("apps.json[{i}].{field}: placeholder {s}"),
                                    ))
                                }
                                Some(s)
                                    if field == "branch"
                                        && matches!(s, "main" | "master" | "develop") =>
                                {
                                    f.push(Finding::new(
                                        Warn,
                                        None,
                                        format!("apps.json[{i}].branch: {s} é móvel"),
                                    ))
                                }
                                Some(_) => {}
                            }
                        }
                    }
                }
            },
        },
    }
    f
}
