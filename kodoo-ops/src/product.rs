use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Product {
    Trivia,
    Accounting,
}

impl Product {
    pub const ALL: [Product; 2] = [Product::Trivia, Product::Accounting];

    pub fn slug(self) -> &'static str {
        match self {
            Product::Trivia => "trivia",
            Product::Accounting => "accounting",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Product::Trivia => "Kodoo Trivia",
            Product::Accounting => "Kodoo Accounting",
        }
    }

    pub fn from_slug(s: &str) -> Option<Product> {
        Product::ALL.into_iter().find(|p| p.slug() == s)
    }

    pub fn other(self) -> Product {
        match self {
            Product::Trivia => Product::Accounting,
            Product::Accounting => Product::Trivia,
        }
    }
}

/// Caminhos de um produto dentro do diretorio `deployment/`.
#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub product: Product,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>, product: Product) -> Self {
        Paths { root: root.into(), product }
    }
    pub fn dir(&self) -> PathBuf {
        self.root.join("products").join(self.product.slug())
    }
    pub fn env(&self) -> PathBuf {
        self.dir().join(".env")
    }
    pub fn env_example(&self) -> PathBuf {
        self.dir().join(".env.example")
    }
    pub fn apps_json(&self) -> PathBuf {
        self.dir().join("apps.json")
    }
    pub fn build_env(&self) -> PathBuf {
        self.dir().join("build.env")
    }
    pub fn dc_sh(&self) -> PathBuf {
        self.root.join("common").join("dc.sh")
    }
    pub fn build_sh(&self) -> PathBuf {
        self.root.join("common").join("build-image.sh")
    }
}

/// Sobe a partir de `start` ate achar um diretorio que contenha `common/compose.base.yaml`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join("common").join("compose.base.yaml").is_file() {
            return Some(dir.to_path_buf());
        }
        // tambem aceita estar ao lado de deployment/
        let sub = dir.join("deployment");
        if sub.join("common").join("compose.base.yaml").is_file() {
            return Some(sub);
        }
        cur = dir.parent();
    }
    None
}
