//! Execucao de comandos em segundo plano, com eventos para a UI.
//!
//! Modelo para UI imediata (SuperLightTUI): chame `Runner::poll()` uma vez por frame e
//! acrescente os `Event::Line` ao seu buffer de log. Nenhuma chamada aqui bloqueia.
use crate::envfile::EnvFile;
use crate::error::{Error, Result};
use crate::product::{Paths, Product};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Valor sensivel: `Debug` nunca o imprime.
#[derive(Clone)]
pub struct Secret(String);
impl Secret {
    pub fn new(s: impl Into<String>) -> Secret {
        Secret(s.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Clone, Debug)]
pub enum Job {
    /// `docker compose <args>` do produto (via common/dc.sh). Ex.: ["up","-d"].
    Compose {
        args: Vec<String>,
    },
    /// common/build-image.sh <produto> <release> (PUSH=1 se `push`).
    Build {
        release: String,
        push: bool,
    },
    /// bench new-site + host_name https. A senha vai por variavel de ambiente, nunca por argv.
    NewSite {
        site: String,
        admin_password: Secret,
    },
    InstallApps {
        site: String,
        apps: Vec<String>,
    },
    /// Imprime um site por linha (stdout).
    ListSites,
    Backup {
        site: String,
    },
    Migrate {
        site: String,
    },
}

#[derive(Clone, Debug)]
pub enum Step {
    Run(Job),
    SetEnv { key: String, value: String },
}

#[derive(Clone, Debug)]
pub struct Task {
    pub product: Product,
    pub step: Step,
}

impl Task {
    pub fn job(product: Product, job: Job) -> Task {
        Task {
            product,
            step: Step::Run(job),
        }
    }
    pub fn set_env(product: Product, key: &str, value: &str) -> Task {
        Task {
            product,
            step: Step::SetEnv {
                key: key.into(),
                value: value.into(),
            },
        }
    }
}

/// Comando pronto para executar (testavel sem rodar nada).
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    /// Textos a mascarar nas linhas de saida.
    pub redact: Vec<String>,
}

fn valid_site(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = s.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

fn valid_app(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_lowercase())
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn valid_release(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn site_arg(site: &str) -> Result<()> {
    if valid_site(site) {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "nome de site invalido: {site:?} (use um hostname minusculo)"
        )))
    }
}

impl Job {
    pub fn title(&self) -> String {
        match self {
            Job::Compose { args } => format!("compose {}", args.join(" ")),
            Job::Build { release, push } => {
                format!("build {release}{}", if *push { " (push)" } else { "" })
            }
            Job::NewSite { site, .. } => format!("novo site {site}"),
            Job::InstallApps { site, apps } => format!("instalar {} em {site}", apps.join(",")),
            Job::ListSites => "listar sites".into(),
            Job::Backup { site } => format!("backup {site}"),
            Job::Migrate { site } => format!("migrate {site}"),
        }
    }

    pub fn command(&self, p: &Paths) -> Result<CommandSpec> {
        let slug = p.product.slug().to_string();
        let ops = |extra: Vec<String>| -> Vec<String> {
            let mut a = vec![slug.clone(), "run".into(), "--rm".into(), "-T".into()];
            a.extend(extra);
            a
        };
        let dc = |args: Vec<String>| CommandSpec {
            program: p.dc_sh(),
            args,
            env: vec![],
            cwd: p.root.clone(),
            redact: vec![],
        };
        let s = |x: &str| x.to_string();

        Ok(match self {
            Job::Compose { args } => {
                if args.is_empty() {
                    return Err(Error::Invalid("compose sem argumentos".into()));
                }
                let mut a = vec![slug.clone()];
                a.extend(args.iter().cloned());
                dc(a)
            }
            Job::Build { release, push } => {
                if !valid_release(release) {
                    return Err(Error::Invalid(format!("release invalida: {release:?}")));
                }
                CommandSpec {
                    program: p.build_sh(),
                    args: vec![slug.clone(), release.clone()],
                    env: if *push { vec![(s("PUSH"), s("1"))] } else { vec![] },
                    cwd: p.root.clone(),
                    redact: vec![],
                }
            }
            Job::NewSite { site, admin_password } => {
                site_arg(site)?;
                let pw = admin_password.expose();
                if pw.len() < 12 {
                    return Err(Error::Invalid("senha do Administrator com menos de 12 caracteres".into()));
                }
                let script = "bench new-site \"$SITE\" --mariadb-user-host-login-scope='%' \
                              --db-root-password \"$DB_PASSWORD\" --admin-password \"$ADMIN_PASSWORD\" \
                              && bench --site \"$SITE\" set-config host_name \"https://$SITE\"";
                let mut spec = dc(ops(vec![
                    s("-e"),
                    format!("SITE={site}"),
                    s("-e"),
                    s("ADMIN_PASSWORD"), // sem valor: herda do ambiente do processo (nao aparece no ps)
                    s("ops"),
                    s("bash"),
                    s("-c"),
                    s(script),
                ]));
                spec.env.push((s("ADMIN_PASSWORD"), pw.to_string()));
                spec.redact.push(pw.to_string());
                spec
            }
            Job::InstallApps { site, apps } => {
                site_arg(site)?;
                if apps.is_empty() || !apps.iter().all(|a| valid_app(a)) {
                    return Err(Error::Invalid("lista de apps vazia ou com nome invalido".into()));
                }
                let mut a = vec![s("ops"), s("bench"), s("--site"), site.clone(), s("install-app")];
                a.extend(apps.iter().cloned());
                dc(ops(a))
            }
            Job::ListSites => dc(ops(vec![
                s("ops"),
                s("bash"),
                s("-c"),
                s("for d in sites/*/; do [ -f \"${d}site_config.json\" ] && basename \"$d\"; done; true"),
            ])),
            Job::Backup { site } => {
                site_arg(site)?;
                dc(ops(vec![s("ops"), s("bench"), s("--site"), site.clone(), s("backup"), s("--with-files")]))
            }
            Job::Migrate { site } => {
                site_arg(site)?;
                dc(ops(vec![s("ops"), s("bench"), s("--site"), site.clone(), s("migrate")]))
            }
        })
    }
}

/// Fluxo de release: preflight -> backup de cada site -> novo IMAGE_REF -> up -d -> migrate de cada site.
pub fn release_pipeline(product: Product, sites: &[String], new_image_ref: &str) -> Vec<Task> {
    let c = |args: &[&str]| Job::Compose {
        args: args.iter().map(|a| a.to_string()).collect(),
    };
    let mut t = vec![Task::job(product, c(&["config", "-q"]))];
    for s in sites {
        t.push(Task::job(product, Job::Backup { site: s.clone() }));
    }
    t.push(Task::set_env(product, "IMAGE_REF", new_image_ref));
    t.push(Task::job(product, c(&["up", "-d", "--remove-orphans"])));
    for s in sites {
        t.push(Task::job(product, Job::Migrate { site: s.clone() }));
    }
    t.push(Task::job(product, c(&["ps"])));
    t
}

// ---- eventos ----------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

#[derive(Clone, Debug)]
pub enum Event {
    Started {
        id: JobId,
        title: String,
    },
    StepStarted {
        id: JobId,
        index: usize,
        total: usize,
        title: String,
    },
    Line {
        id: JobId,
        stream: Stream,
        text: String,
    },
    Finished {
        id: JobId,
        ok: bool,
        code: Option<i32>,
    },
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for n in it.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn redact(mut line: String, secrets: &[String]) -> String {
    for s in secrets {
        if !s.is_empty() {
            line = line.replace(s.as_str(), "\u{2022}\u{2022}\u{2022}\u{2022}");
        }
    }
    line
}

fn pump<R: Read + Send + 'static>(
    id: JobId,
    stream: Stream,
    r: R,
    tx: Sender<Event>,
    secrets: Arc<Vec<String>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(r).lines().map_while(|l| l.ok()) {
            let text = redact(strip_ansi(&line), &secrets);
            if tx.send(Event::Line { id, stream, text }).is_err() {
                break;
            }
        }
    })
}

pub struct Runner {
    root: PathBuf,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    next: AtomicU64,
    running: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
}

impl Runner {
    pub fn new(root: impl Into<PathBuf>) -> Runner {
        let (tx, rx) = channel();
        Runner {
            root: root.into(),
            tx,
            rx,
            next: AtomicU64::new(1),
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Eventos pendentes (nao bloqueia). Chame uma vez por frame.
    pub fn poll(&self) -> Vec<Event> {
        self.rx.try_iter().collect()
    }

    pub fn is_running(&self, id: JobId) -> bool {
        self.running
            .lock()
            .map(|m| m.contains_key(&id.0))
            .unwrap_or(false)
    }

    pub fn any_running(&self) -> bool {
        self.running.lock().map(|m| !m.is_empty()).unwrap_or(false)
    }

    /// Pede o encerramento do processo atual; o pipeline para.
    pub fn cancel(&self, id: JobId) {
        if let Ok(m) = self.running.lock() {
            if let Some(flag) = m.get(&id.0) {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn spawn_job(&self, product: Product, job: Job) -> JobId {
        let title = job.title();
        self.spawn(&title, vec![Task::job(product, job)])
    }

    pub fn spawn(&self, title: &str, tasks: Vec<Task>) -> JobId {
        let id = JobId(self.next.fetch_add(1, Ordering::SeqCst));
        let cancel = Arc::new(AtomicBool::new(false));
        if let Ok(mut m) = self.running.lock() {
            m.insert(id.0, cancel.clone());
        }
        let _ = self.tx.send(Event::Started {
            id,
            title: title.to_string(),
        });
        let (tx, root, running) = (self.tx.clone(), self.root.clone(), self.running.clone());
        thread::spawn(move || {
            let (ok, code) = run_tasks(&root, &tx, id, &cancel, &tasks);
            if let Ok(mut m) = running.lock() {
                m.remove(&id.0);
            }
            let _ = tx.send(Event::Finished { id, ok, code });
        });
        id
    }
}

fn err_line(tx: &Sender<Event>, id: JobId, text: String) {
    let _ = tx.send(Event::Line {
        id,
        stream: Stream::Err,
        text,
    });
}

fn run_tasks(
    root: &PathBuf,
    tx: &Sender<Event>,
    id: JobId,
    cancel: &AtomicBool,
    tasks: &[Task],
) -> (bool, Option<i32>) {
    let total = tasks.len();
    let mut last_code = Some(0);
    for (index, task) in tasks.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            err_line(tx, id, "[cancelado]".into());
            return (false, None);
        }
        let paths = Paths::new(root.clone(), task.product);
        match &task.step {
            Step::SetEnv { key, value } => {
                let _ = tx.send(Event::StepStarted {
                    id,
                    index,
                    total,
                    title: format!("env {key}"),
                });
                let res = EnvFile::load(&paths.env()).and_then(|mut e| {
                    e.set(key, value)?;
                    e.save(&paths.env())
                });
                match res {
                    Ok(()) => {
                        let _ = tx.send(Event::Line {
                            id,
                            stream: Stream::Out,
                            text: format!("[env] {key} atualizado"),
                        });
                    }
                    Err(e) => {
                        err_line(tx, id, format!("[env] falha: {e}"));
                        return (false, None);
                    }
                }
            }
            Step::Run(job) => {
                let _ = tx.send(Event::StepStarted {
                    id,
                    index,
                    total,
                    title: job.title(),
                });
                let spec = match job.command(&paths) {
                    Ok(s) => s,
                    Err(e) => {
                        err_line(tx, id, format!("[erro] {e}"));
                        return (false, None);
                    }
                };
                let mut cmd = Command::new(&spec.program);
                cmd.args(&spec.args)
                    .envs(spec.env.iter().map(|(k, v)| (k, v)))
                    .current_dir(&spec.cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                let mut child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        err_line(
                            tx,
                            id,
                            format!(
                                "[erro] nao foi possivel iniciar {}: {e}",
                                spec.program.display()
                            ),
                        );
                        return (false, None);
                    }
                };
                let secrets = Arc::new(spec.redact.clone());
                let mut readers = Vec::new();
                if let Some(o) = child.stdout.take() {
                    readers.push(pump(id, Stream::Out, o, tx.clone(), secrets.clone()));
                }
                if let Some(e) = child.stderr.take() {
                    readers.push(pump(id, Stream::Err, e, tx.clone(), secrets.clone()));
                }
                let status = loop {
                    if cancel.load(Ordering::SeqCst) {
                        let _ = child.kill();
                    }
                    match child.try_wait() {
                        Ok(Some(st)) => break Some(st),
                        Ok(None) => thread::sleep(Duration::from_millis(40)),
                        Err(_) => break None,
                    }
                };
                for r in readers {
                    let _ = r.join();
                }
                let code = status.and_then(|s| s.code());
                last_code = code;
                if !status.map_or(false, |s| s.success()) {
                    return (false, code);
                }
            }
        }
    }
    (true, last_code)
}

/// Estado de um servico (de `docker compose ps --format json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceStatus {
    pub service: String,
    pub state: String,
    pub health: String,
}

/// Aceita uma linha JSON por container (compose >= 2.21) ou um array unico.
pub fn parse_ps(text: &str) -> Vec<ServiceStatus> {
    let mut vals: Vec<serde_json::Value> = Vec::new();
    let t = text.trim();
    if t.starts_with('[') {
        if let Ok(serde_json::Value::Array(a)) = serde_json::from_str(t) {
            vals = a;
        }
    } else {
        vals = t
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
    }
    let g = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let mut out: Vec<ServiceStatus> = vals
        .iter()
        .map(|v| ServiceStatus {
            service: g(v, "Service"),
            state: g(v, "State"),
            health: g(v, "Health"),
        })
        .collect();
    out.sort_by(|a, b| a.service.cmp(&b.service));
    out
}
