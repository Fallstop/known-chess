//! Project configuration, shared by `kc-process` and `kc-server`.
//!
//! A single `config.toml` (see the repo root) describes where lichess dumps are
//! downloaded, where the combined book lives, and where to fetch the dump list.
//! `kc-process` downloads dumps into `[storage].downloads` and folds each one
//! into the single combined book at `[storage].book`; `kc-server` reads that one
//! book. Both binaries discover the config the same way — an explicit path, the
//! `KC_CONFIG` env var, or the nearest `config.toml` walking up from the cwd.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The default lichess standard-dump index, used when `[source].list_url` is
/// omitted.
pub const DEFAULT_LIST_URL: &str = "https://database.lichess.org/standard/list.txt";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub storage: Storage,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub server: Server,
    /// The file this config was loaded from (not part of the TOML).
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Storage {
    /// Directory holding downloaded `.pgn.zst` dumps.
    pub downloads: PathBuf,
    /// The combined book every dump is folded into. Defaults to
    /// `<downloads>/known.book` (see [`Storage::book_path`]).
    #[serde(default)]
    pub book: Option<PathBuf>,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            downloads: PathBuf::from("data"),
            book: None,
        }
    }
}

impl Storage {
    /// The combined book path, falling back to `<downloads>/known.book`.
    pub fn book_path(&self) -> PathBuf {
        self.book
            .clone()
            .unwrap_or_else(|| self.downloads.join("known.book"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    #[serde(default = "default_list_url")]
    pub list_url: String,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            list_url: default_list_url(),
        }
    }
}

fn default_list_url() -> String {
    DEFAULT_LIST_URL.to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Book to serve. Defaults to `[storage].book` (the combined book).
    #[serde(default)]
    pub book: Option<PathBuf>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            book: None,
        }
    }
}

fn default_bind() -> String {
    "0.0.0.0:8080".to_string()
}

#[derive(Debug)]
pub enum ConfigError {
    NotFound,
    Read(PathBuf, std::io::Error),
    Parse(PathBuf, toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NotFound => write!(
                f,
                "no config.toml found (set KC_CONFIG or create one in the project root)"
            ),
            ConfigError::Read(p, e) => write!(f, "reading {}: {e}", p.display()),
            ConfigError::Parse(p, e) => write!(f, "parsing {}: {e}", p.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Load config from an explicit path, or discover one. Resolution order:
    /// `explicit` arg → `KC_CONFIG` env → nearest `config.toml` from the cwd up.
    pub fn load(explicit: Option<&Path>) -> Result<Self, ConfigError> {
        let path = match explicit {
            Some(p) => p.to_path_buf(),
            None => match std::env::var_os("KC_CONFIG") {
                Some(p) => PathBuf::from(p),
                None => Self::discover().ok_or(ConfigError::NotFound)?,
            },
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| ConfigError::Read(path.clone(), e))?;
        let mut cfg: Config =
            toml::from_str(&text).map_err(|e| ConfigError::Parse(path.clone(), e))?;
        cfg.path = path;
        Ok(cfg)
    }

    /// Walk up from the current directory looking for a `config.toml`.
    pub fn discover() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join("config.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Path a downloaded dump with the given filename should live at.
    pub fn download_path(&self, filename: &str) -> PathBuf {
        self.storage.downloads.join(filename)
    }

    /// The combined book the processor writes and the server reads.
    pub fn book_path(&self) -> PathBuf {
        self.storage.book_path()
    }

    /// The book the server should load (defaults to the combined book).
    pub fn server_book_path(&self) -> PathBuf {
        self.server.book.clone().unwrap_or_else(|| self.book_path())
    }

    /// Sidecar listing the dump filenames already folded into the combined book,
    /// so `list` can show what's been processed. Lives next to the book.
    pub fn manifest_path(&self) -> PathBuf {
        let book = self.book_path();
        let mut name = book.file_name().map(|f| f.to_os_string()).unwrap_or_default();
        name.push(".sources");
        book.with_file_name(name)
    }

    /// Read the set of dump filenames already merged into the combined book.
    pub fn read_manifest(&self) -> Vec<String> {
        match std::fs::read_to_string(self.manifest_path()) {
            Ok(text) => text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}
