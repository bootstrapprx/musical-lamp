//! kodoo-ops: nucleo funcional (sem UI) para deploy e ciclo de vida do .env.
//!
//! A UI (SuperLightTUI, gerada no tui.builders) so precisa de `Ops`:
//!   * estado/acoes sincronas e rapidas: `env_*`, `validate`, `generate`
//!   * acoes lentas (docker, bench): `run`/`release` devolvem um `JobId` e a UI chama
//!     `poll()` uma vez por frame para receber `Event`s.
pub mod app;
pub mod envfile;
pub mod error;
pub mod product;
pub mod runner;
pub mod secrets;
pub mod validate;

pub use app::AppState;
pub use envfile::EnvFile;
pub use error::{Error, Result};
pub use product::{find_root, Paths, Product};
pub use runner::{release_pipeline, Event, Job, JobId, Runner, Secret, ServiceStatus, Stream, Task};
pub use secrets::SecretKind;
pub use validate::{Finding, Severity};

use std::path::PathBuf;

pub struct Ops {
    pub root: PathBuf,
    pub runner: Runner,
}

impl Ops {
    /// `root` = diretorio `deployment/` (o que contem common/ e products/).
    pub fn open(root: impl Into<PathBuf>) -> Result<Ops> {
        let root: PathBuf = root.into();
        if !root.join("common").join("compose.base.yaml").is_file() {
            return Err(Error::NotFound(format!("{} nao parece o diretorio deployment/", root.display())));
        }
        Ok(Ops { runner: Runner::new(root.clone()), root })
    }

    pub fn paths(&self, p: Product) -> Paths {
        Paths::new(self.root.clone(), p)
    }

    // ---- .env ---------------------------------------------------------------
    pub fn env_exists(&self, p: Product) -> bool {
        self.paths(p).env().is_file()
    }

    pub fn env_load(&self, p: Product) -> Result<EnvFile> {
        EnvFile::load(&self.paths(p).env())
    }

    pub fn env_save(&self, p: Product, env: &EnvFile) -> Result<()> {
        env.save(&self.paths(p).env())
    }

    /// Cria o .env a partir do .env.example (0600) e gera DB_PASSWORD. Nao sobrescreve.
    pub fn env_init(&self, p: Product) -> Result<EnvFile> {
        let paths = self.paths(p);
        if paths.env().exists() {
            return Err(Error::Invalid(format!("{} ja existe", paths.env().display())));
        }
        let mut env = EnvFile::load(&paths.env_example())?;
        if env.is_blank("DB_PASSWORD") {
            env.set("DB_PASSWORD", &secrets::generate(SecretKind::Hex(24))?)?;
        }
        env.save(&paths.env())?;
        Ok(env)
    }

    pub fn generate(&self, kind: SecretKind) -> Result<String> {
        secrets::generate(kind)
    }

    /// Todas as verificacoes do produto: .env + deriva do exemplo + pre-requisitos de build.
    pub fn validate(&self, p: Product) -> Vec<Finding> {
        let paths = self.paths(p);
        let mut out = Vec::new();
        match EnvFile::load(&paths.env()) {
            Err(_) => out.push(Finding {
                severity: Severity::Error,
                key: None,
                message: format!("{} nao existe (rode env_init)", paths.env().display()),
            }),
            Ok(env) => {
                let other = self.env_load(p.other()).ok();
                out.extend(validate::validate_env(&env, &paths.env(), other.as_ref()));
                if let Ok(ex) = EnvFile::load(&paths.env_example()) {
                    out.extend(validate::drift(&env, &ex));
                }
            }
        }
        out.extend(validate::validate_build(&paths));
        out.sort_by(|a, b| b.severity.cmp(&a.severity));
        out
    }

    // ---- tarefas em segundo plano ----------------------------------------------
    pub fn run(&self, p: Product, job: Job) -> JobId {
        self.runner.spawn_job(p, job)
    }

    pub fn release(&self, p: Product, sites: &[String], new_image_ref: &str) -> JobId {
        let title = format!("release {} -> {new_image_ref}", p.slug());
        self.runner.spawn(&title, release_pipeline(p, sites, new_image_ref))
    }

    /// Chame uma vez por frame.
    pub fn poll(&self) -> Vec<Event> {
        self.runner.poll()
    }
}
