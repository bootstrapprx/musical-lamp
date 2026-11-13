//! `AppState`: modelo de visao SEM dependencia de UI. O desenho (SuperLightTUI) apenas
//! le estes campos e chama estes metodos. Chame `tick()` uma vez por frame.
use crate::envfile::EnvFile;
use crate::error::Result;
use crate::runner::{parse_ps, Event, Job, JobId, Secret, ServiceStatus, Stream};
use crate::secrets::{self, SecretKind};
use crate::validate::{Finding, Severity};
use crate::{Ops, Product};
use std::collections::VecDeque;
use std::path::PathBuf;

const MAX_LOG: usize = 2000;

#[derive(Clone, Debug)]
pub struct LogLine {
    pub is_err: bool,
    pub text: String,
}

/// Uma linha do editor de .env, pronta para exibir.
#[derive(Clone, Debug)]
pub struct Field {
    pub key: String,
    /// Valor real (para o campo de edicao).
    pub value: String,
    /// Valor para exibir (mascarado se for segredo e `reveal` estiver desligado).
    pub display: String,
    pub is_secret: bool,
    pub has_error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Generic,
    Sites,
    Ps,
}

pub struct AppState {
    pub ops: Ops,
    pub product: Product,
    pub env: Option<EnvFile>,
    pub dirty: bool,
    pub reveal_secrets: bool,
    pub fields: Vec<Field>,
    pub findings: Vec<Finding>,
    pub log: VecDeque<LogLine>,
    pub status: Vec<ServiceStatus>,
    pub sites: Vec<String>,
    /// Job em andamento e progresso (indice base 0, total, titulo do passo).
    pub active: Option<JobId>,
    pub step: Option<(usize, usize, String)>,
    pub last_ok: Option<bool>,
    /// Mensagem curta para toast/rodape (avisos de rotacao de segredo, erros de entrada...).
    pub message: Option<String>,
    /// Segredo gerado que deve ser mostrado UMA vez (ex.: senha do Administrator).
    pub show_once: Option<String>,
    pending: Pending,
    capture: Vec<String>,
}

impl AppState {
    pub fn new(root: impl Into<PathBuf>) -> Result<AppState> {
        let mut s = AppState {
            ops: Ops::open(root)?,
            product: Product::Trivia,
            env: None,
            dirty: false,
            reveal_secrets: false,
            fields: vec![],
            findings: vec![],
            log: VecDeque::new(),
            status: vec![],
            sites: vec![],
            active: None,
            step: None,
            last_ok: None,
            message: None,
            show_once: None,
            pending: Pending::Generic,
            capture: vec![],
        };
        s.load();
        Ok(s)
    }

    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    // ---- produto / .env -------------------------------------------------------
    pub fn select_product(&mut self, index: usize) {
        if let Some(p) = Product::ALL.get(index) {
            self.product = *p;
            self.sites.clear();
            self.status.clear();
            self.load();
        }
    }

    pub fn load(&mut self) {
        self.env = self.ops.env_load(self.product).ok();
        self.dirty = false;
        self.rebuild_fields();
        self.validate();
    }

    /// Cria o .env a partir do exemplo (gera DB_PASSWORD).
    pub fn init_env(&mut self) {
        match self.ops.env_init(self.product) {
            Ok(_) => self.load(),
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub fn toggle_reveal(&mut self) {
        self.reveal_secrets = !self.reveal_secrets;
        self.rebuild_fields();
    }

    pub fn edit(&mut self, key: &str, value: &str) {
        if let Some(env) = self.env.as_mut() {
            match env.set(key, value) {
                Ok(()) => {
                    self.dirty = true;
                    self.rebuild_fields();
                }
                Err(e) => self.message = Some(e.to_string()),
            }
        }
    }

    /// Gera um segredo para `key` (padrao: hex de 24 bytes) e avisa sobre rotacao.
    pub fn generate(&mut self, key: &str) {
        let kind = secrets::default_kind_for(key).unwrap_or(SecretKind::Hex(24));
        match secrets::generate(kind) {
            Ok(v) => {
                self.edit(key, &v);
                self.message = secrets::rotation_note(key).map(String::from);
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }

    pub fn save(&mut self) {
        if let Some(env) = &self.env {
            match self.ops.env_save(self.product, env) {
                Ok(()) => {
                    self.dirty = false;
                    self.message = Some("salvo (backup .env.bak-* criado)".into());
                    self.validate();
                }
                Err(e) => self.message = Some(e.to_string()),
            }
        }
    }

    pub fn validate(&mut self) {
        self.findings = self.ops.validate(self.product);
        self.rebuild_fields();
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    fn rebuild_fields(&mut self) {
        let Some(env) = &self.env else {
            self.fields.clear();
            return;
        };
        self.fields = env
            .keys()
            .into_iter()
            .map(|k| {
                let v = env.get(k).unwrap_or("").to_string();
                let is_secret = secrets::is_secret_key(k);
                let display = if is_secret && !self.reveal_secrets {
                    secrets::mask(&v)
                } else {
                    v.clone()
                };
                let has_error = self
                    .findings
                    .iter()
                    .any(|f| f.severity == Severity::Error && f.key.as_deref() == Some(k));
                Field {
                    key: k.to_string(),
                    value: v,
                    display,
                    is_secret,
                    has_error,
                }
            })
            .collect();
    }

    // ---- acoes em segundo plano --------------------------------------------------
    fn start(&mut self, job: Job, pending: Pending) {
        if self.busy() {
            self.message = Some("ja existe uma tarefa em andamento".into());
            return;
        }
        self.pending = pending;
        self.capture.clear();
        self.last_ok = None;
        self.active = Some(self.ops.run(self.product, job));
    }

    pub fn refresh_status(&mut self) {
        self.start(
            Job::Compose {
                args: vec!["ps".into(), "-a".into(), "--format".into(), "json".into()],
            },
            Pending::Ps,
        );
    }
    pub fn refresh_sites(&mut self) {
        self.start(Job::ListSites, Pending::Sites);
    }
    pub fn build(&mut self, release: &str, push: bool) {
        self.start(
            Job::Build {
                release: release.into(),
                push,
            },
            Pending::Generic,
        );
    }
    pub fn compose(&mut self, args: &[&str]) {
        self.start(
            Job::Compose {
                args: args.iter().map(|s| s.to_string()).collect(),
            },
            Pending::Generic,
        );
    }
    /// Gera a senha do Administrator (exposta em `show_once`) e cria o site.
    pub fn new_site(&mut self, site: &str) {
        match secrets::alnum(24) {
            Ok(pw) => {
                self.show_once = Some(pw.clone());
                self.start(
                    Job::NewSite {
                        site: site.into(),
                        admin_password: Secret::new(pw),
                    },
                    Pending::Generic,
                );
            }
            Err(e) => self.message = Some(e.to_string()),
        }
    }
    pub fn install_apps(&mut self, site: &str, apps_csv: &str) {
        let apps = apps_csv
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        self.start(
            Job::InstallApps {
                site: site.into(),
                apps,
            },
            Pending::Generic,
        );
    }
    pub fn migrate(&mut self, site: &str) {
        self.start(Job::Migrate { site: site.into() }, Pending::Generic);
    }
    pub fn backup(&mut self, site: &str) {
        self.start(Job::Backup { site: site.into() }, Pending::Generic);
    }
    /// preflight -> backup de todos os `sites` -> novo IMAGE_REF -> up -> migrate.
    pub fn release(&mut self, image_ref: &str) {
        if self.busy() {
            self.message = Some("ja existe uma tarefa em andamento".into());
            return;
        }
        if self.sites.is_empty() {
            self.message = Some("liste os sites antes (refresh_sites)".into());
            return;
        }
        self.pending = Pending::Generic;
        self.last_ok = None;
        self.active = Some(
            self.ops
                .release(self.product, &self.sites.clone(), image_ref),
        );
    }
    pub fn cancel(&mut self) {
        if let Some(id) = self.active {
            self.ops.runner.cancel(id);
        }
    }
    pub fn clear_log(&mut self) {
        self.log.clear();
    }

    // ---- chamar UMA vez por frame -----------------------------------------------
    pub fn tick(&mut self) {
        for ev in self.ops.poll() {
            match ev {
                Event::Started { title, .. } => self.push_log(false, format!("== {title}")),
                Event::StepStarted {
                    index,
                    total,
                    title,
                    ..
                } => {
                    self.step = Some((index, total, title.clone()));
                    self.push_log(false, format!("-- [{}/{total}] {title}", index + 1));
                }
                Event::Line { stream, text, .. } => {
                    if stream == Stream::Out && self.pending != Pending::Generic {
                        self.capture.push(text.clone());
                    }
                    // so mostra no log o que nao e captura estruturada
                    if self.pending == Pending::Generic || stream == Stream::Err {
                        self.push_log(stream == Stream::Err, text);
                    }
                }
                Event::Finished { ok, code, .. } => {
                    self.active = None;
                    self.step = None;
                    self.last_ok = Some(ok);
                    match self.pending {
                        Pending::Sites if ok => {
                            self.sites = self
                                .capture
                                .iter()
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        Pending::Ps if ok => self.status = parse_ps(&self.capture.join("\n")),
                        _ => {}
                    }
                    self.pending = Pending::Generic;
                    self.capture.clear();
                    self.push_log(
                        !ok,
                        format!("== {} (codigo {code:?})", if ok { "ok" } else { "FALHOU" }),
                    );
                    // um SetEnv de release pode ter mudado o .env em disco
                    if self.env.is_some() && !self.dirty {
                        self.load();
                    }
                }
            }
        }
    }

    fn push_log(&mut self, is_err: bool, text: String) {
        if self.log.len() >= MAX_LOG {
            self.log.pop_front();
        }
        self.log.push_back(LogLine { is_err, text });
    }
}
