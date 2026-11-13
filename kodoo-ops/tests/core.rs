use kodoo_ops::runner::{parse_ps, strip_ansi, Job, Secret};
use kodoo_ops::{secrets, validate, EnvFile, Ops, Paths, Product, SecretKind, Severity};
use std::path::PathBuf;

fn deployment() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../deployment");
    p.join("common/compose.base.yaml").is_file().then_some(p)
}

fn good_env() -> EnvFile {
    EnvFile::parse(
        "IMAGE_REF=kodoo/trivia:2026.11.1\nDB_PASSWORD=0123456789abcdef0123456789abcdef0123456789abcdef\n\
         TUNNEL_TOKEN=eyJhIjoiMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAiLCJ0IjoiMTIzNDU2Nzg5MCJ9\n\
         CLOUDFLARED_VERSION=2026.1.0\nEDGE_SUBNET=172.30.1.0/28\nEDGE_IP_RANGE=172.30.1.8/29\nCLOUDFLARED_IP=172.30.1.2\n",
    )
}

fn errs(f: &[kodoo_ops::Finding]) -> Vec<String> {
    f.iter().filter(|x| x.severity == Severity::Error).map(|x| x.message.clone()).collect()
}

#[test]
fn good_env_has_no_errors() {
    let f = validate::validate_env(&good_env(), std::path::Path::new("/nonexistent"), None);
    assert!(errs(&f).is_empty(), "{f:?}");
}

#[test]
fn catches_placeholders_and_latest() {
    let mut e = good_env();
    e.set("IMAGE_REF", "kodoo/trivia:RELEASE").unwrap();
    e.set("CLOUDFLARED_VERSION", "latest").unwrap();
    e.set("TUNNEL_TOKEN", "").unwrap();
    let m = errs(&validate::validate_env(&e, std::path::Path::new("/x"), None)).join("|");
    assert!(m.contains("RELEASE") && m.contains("latest") && m.contains("TUNNEL_TOKEN"), "{m}");
}

#[test]
fn cloudflared_ip_rules() {
    for (ip, needle) in [("172.30.1.1", "gateway"), ("172.30.1.9", "EDGE_IP_RANGE"), ("172.30.2.2", "fora"), ("172.30.1.0", "rede")] {
        let mut e = good_env();
        e.set("CLOUDFLARED_IP", ip).unwrap();
        let m = errs(&validate::validate_env(&e, std::path::Path::new("/x"), None)).join("|");
        assert!(m.contains(needle), "{ip}: {m}");
    }
}

#[test]
fn subnet_collision_between_products() {
    let a = good_env();
    let mut b = good_env();
    b.set("EDGE_SUBNET", "172.30.1.0/24").unwrap();
    let m = errs(&validate::validate_env(&a, std::path::Path::new("/x"), Some(&b))).join("|");
    assert!(m.contains("colide"), "{m}");
}

#[test]
fn digest_is_accepted_without_warning() {
    let mut e = good_env();
    e.set("IMAGE_REF", &format!("kodoo/trivia@sha256:{}", "a".repeat(64))).unwrap();
    let f = validate::validate_env(&e, std::path::Path::new("/x"), None);
    assert!(!f.iter().any(|x| x.key.as_deref() == Some("IMAGE_REF")), "{f:?}");
}

#[test]
fn secrets_shapes() {
    assert_eq!(secrets::generate(SecretKind::Hex(24)).unwrap().len(), 48);
    let a = secrets::generate(SecretKind::Alnum(40)).unwrap();
    assert_eq!(a.len(), 40);
    assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    let u = secrets::generate(SecretKind::UrlSafe(32)).unwrap();
    assert_eq!(u.len(), 43); // 32 bytes -> 43 chars sem padding
    assert!(u.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    assert_ne!(secrets::hex(16).unwrap(), secrets::hex(16).unwrap());
    assert!(secrets::generate(SecretKind::Hex(0)).is_err());
}

#[test]
fn new_site_password_never_in_argv() {
    let paths = Paths::new("/srv/deployment", Product::Trivia);
    let pw = "S3nhaSuperSecreta99";
    let spec = Job::NewSite { site: "trivia.kodoo.online".into(), admin_password: Secret::new(pw) }
        .command(&paths)
        .unwrap();
    assert!(!spec.args.iter().any(|a| a.contains(pw)), "senha vazou no argv");
    assert!(spec.env.iter().any(|(k, v)| k == "ADMIN_PASSWORD" && v == pw));
    assert_eq!(spec.redact, vec![pw.to_string()]);
    assert_eq!(spec.args[0], "trivia");
    assert!(spec.args.windows(2).any(|w| w == ["-e", "ADMIN_PASSWORD"]));
    assert!(spec.args.contains(&"-T".to_string()));
    assert_eq!(format!("{:?}", Secret::new(pw)), "Secret(***)");
}

#[test]
fn rejects_bad_inputs() {
    let paths = Paths::new("/srv/deployment", Product::Accounting);
    for site in ["", "Foo.com", "a b.com", "x;rm -rf /.com", "semponto", "-a.com", "a..com"] {
        assert!(Job::Migrate { site: site.into() }.command(&paths).is_err(), "{site:?}");
    }
    assert!(Job::InstallApps { site: "a.com".into(), apps: vec!["lms;x".into()] }.command(&paths).is_err());
    assert!(Job::InstallApps { site: "a.com".into(), apps: vec![] }.command(&paths).is_err());
    assert!(Job::Build { release: "1.0 x".into(), push: false }.command(&paths).is_err());
    assert!(Job::Compose { args: vec![] }.command(&paths).is_err());
    assert!(Job::NewSite { site: "a.com".into(), admin_password: Secret::new("curta") }.command(&paths).is_err());
}

#[test]
fn migrate_and_build_commands() {
    let paths = Paths::new("/srv/deployment", Product::Accounting);
    let m = Job::Migrate { site: "cli.exemplo.com".into() }.command(&paths).unwrap();
    assert!(m.program.ends_with("common/dc.sh"));
    assert_eq!(m.args, ["accounting", "run", "--rm", "-T", "ops", "bench", "--site", "cli.exemplo.com", "migrate"]);
    let b = Job::Build { release: "2026.11.1".into(), push: true }.command(&paths).unwrap();
    assert!(b.program.ends_with("common/build-image.sh"));
    assert_eq!(b.env, vec![("PUSH".to_string(), "1".to_string())]);
}

#[test]
fn release_pipeline_order() {
    use kodoo_ops::runner::{release_pipeline, Step};
    let t = release_pipeline(Product::Trivia, &["a.com".into(), "b.com".into()], "kodoo/trivia@sha256:x");
    let kinds: Vec<String> = t
        .iter()
        .map(|t| match &t.step {
            Step::Run(j) => j.title(),
            Step::SetEnv { key, .. } => format!("env {key}"),
        })
        .collect();
    assert_eq!(
        kinds,
        ["compose config -q", "backup a.com", "backup b.com", "env IMAGE_REF", "compose up -d --remove-orphans", "migrate a.com", "migrate b.com", "compose ps"]
    );
}

#[test]
fn ps_parsing_both_formats() {
    let lines = "{\"Service\":\"db\",\"State\":\"running\",\"Health\":\"healthy\"}\n{\"Service\":\"backend\",\"State\":\"running\",\"Health\":\"\"}\n";
    let arr = "[{\"Service\":\"db\",\"State\":\"running\",\"Health\":\"healthy\"}]";
    assert_eq!(parse_ps(lines).len(), 2);
    assert_eq!(parse_ps(lines)[0].service, "backend");
    assert_eq!(parse_ps(arr)[0].health, "healthy");
    assert!(parse_ps("").is_empty());
}

#[test]
fn ansi_stripped() {
    assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m\r"), "ok");
}

#[test]
fn save_is_private_with_backup_rotation() {
    let dir = std::env::temp_dir().join(format!("kodoo-ops-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".env");
    let mut e = EnvFile::parse("A=1\n");
    e.save(&path).unwrap();
    for i in 0..8 {
        e.set("A", &i.to_string()).unwrap();
        e.save(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        if i == 2 { break; }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(EnvFile::load(&path).unwrap().get("A"), Some("2"));
    let baks = std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains(".bak-")).count();
    assert!(baks >= 1 && baks <= 5);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- contra os arquivos reais de ../deployment ---------------------------------
#[test]
fn real_examples_roundtrip_and_flags() {
    let Some(root) = deployment() else { return };
    for p in Product::ALL {
        let paths = Paths::new(&root, p);
        let text = std::fs::read_to_string(paths.env_example()).unwrap();
        assert_eq!(EnvFile::parse(&text).to_text(), text, "{p:?} roundtrip");
    }
}

#[test]
fn real_examples_are_not_ready_until_filled() {
    let Some(root) = deployment() else { return };
    let ops = Ops::open(&root).unwrap();
    // O example tem EDGE_* validos e distintos entre produtos; so faltam segredos/release.
    for p in Product::ALL {
        let ex = EnvFile::load(&ops.paths(p).env_example()).unwrap();
        let other = EnvFile::load(&ops.paths(p.other()).env_example()).unwrap();
        let m = errs(&validate::validate_env(&ex, std::path::Path::new("/x"), Some(&other))).join("|");
        assert!(!m.contains("colide") && !m.contains("gateway") && !m.contains("EDGE_IP_RANGE: fora"), "{p:?}: {m}");
        assert!(m.contains("TUNNEL_TOKEN") && m.contains("RELEASE"), "{p:?}: {m}");
    }
    // build: placeholders de apps.json e FRAPPE_REF vazio sao erros
    let b = errs(&validate::validate_build(&ops.paths(Product::Trivia))).join("|");
    assert!(b.contains("vazio em build.env") && b.contains("placeholder"), "{b}");
}

#[test]
fn env_init_generates_db_password() {
    let Some(root) = deployment() else { return };
    // copia para um diretorio temporario para nao sujar o deployment real
    let tmp = std::env::temp_dir().join(format!("kodoo-ops-init-{}", std::process::id()));
    for sub in ["common", "products/trivia"] {
        std::fs::create_dir_all(tmp.join(sub)).unwrap();
    }
    std::fs::copy(root.join("common/compose.base.yaml"), tmp.join("common/compose.base.yaml")).unwrap();
    std::fs::copy(root.join("products/trivia/.env.example"), tmp.join("products/trivia/.env.example")).unwrap();
    let ops = Ops::open(&tmp).unwrap();
    let env = ops.env_init(Product::Trivia).unwrap();
    assert_eq!(env.get("DB_PASSWORD").unwrap().len(), 48);
    assert!(ops.env_init(Product::Trivia).is_err(), "nao deve sobrescrever");
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn app_state_tick_captures_sites_and_logs() {
    use kodoo_ops::AppState;
    let tmp = std::env::temp_dir().join(format!("kodoo-ops-app-{}", std::process::id()));
    for sub in ["common", "products/trivia", "products/accounting"] {
        std::fs::create_dir_all(tmp.join(sub)).unwrap();
    }
    std::fs::write(tmp.join("common/compose.base.yaml"), "services: {}\n").unwrap();
    // dc.sh de mentira: lista dois sites
    std::fs::write(
        tmp.join("common/dc.sh"),
        "#!/usr/bin/env bash\nif [[ \"$*\" == *site_config* ]]; then printf 'a.exemplo.com\\nb.exemplo.com\\n'; else echo \"args: $*\"; fi\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.join("common/dc.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(tmp.join("products/trivia/.env.example"), "DB_PASSWORD=\nTUNNEL_TOKEN=\n").unwrap();
    let mut app = AppState::new(&tmp).unwrap();
    app.init_env();
    assert_eq!(app.fields.len(), 2);
    assert!(app.fields[0].is_secret && app.fields[0].display.contains('\u{2022}'));
    app.toggle_reveal();
    assert_eq!(app.fields[0].display.len(), 48);

    app.refresh_sites();
    let mut guard = 0;
    while app.busy() && guard < 200 {
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(20));
        guard += 1;
    }
    app.tick();
    assert_eq!(app.sites, vec!["a.exemplo.com", "b.exemplo.com"]);
    assert_eq!(app.last_ok, Some(true));

    app.compose(&["ps"]);
    while app.busy() {
        app.tick();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    app.tick();
    assert!(app.log.iter().any(|l| l.text.contains("args: trivia ps")));
    std::fs::remove_dir_all(&tmp).ok();
}
