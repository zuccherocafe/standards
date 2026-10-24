//! Resolves domain aliases and serves files contained by the selected domain root.

use std::{fs, path::Component};

use camino::Utf8PathBuf;

use crate::config::Config;

/// File bytes and media type ready for an HTTP adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// Original file bytes.
    pub body: Vec<u8>,
    /// Media type of the returned file.
    pub content_type: &'static str,
}

/// Static file handler for one node.
#[derive(Clone, Debug)]
pub struct WebServer {
    config: Config,
}

impl WebServer {
    /// Construct a server from parsed configuration.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self { config }
    }

    /// Resolve a domain and read the requested file; directories use `index.html`.
    ///
    /// Query parameters do not affect static file serving.
    ///
    /// # Errors
    /// Returns [`ServeError`] for unsafe paths or unavailable files.
    pub fn serve(&self, host: &str, path: &str, _query: &str) -> Result<Response, ServeError> {
        let file = self.resolve(host, path)?;
        let body = fs::read(&file).map_err(|source| file_error(&file, source))?;
        let content_type = match file.extension() {
            Some(extension) if extension.eq_ignore_ascii_case("png") => "image/png",
            Some("html") => "text/html",
            Some("css") => "text/css",
            Some("txt") => "text/plain",
            _ => "application/octet-stream",
        };
        Ok(Response { body, content_type })
    }

    fn resolve(&self, host: &str, path: &str) -> Result<Utf8PathBuf, ServeError> {
        let hostname = host.split_once(':').map_or(host, |(hostname, _)| hostname);
        let domain = self
            .config
            .domains
            .get(hostname)
            .map_or(hostname, String::as_str);
        if domain.is_empty() || domain.contains(['/', '\\']) || matches!(domain, "." | "..") {
            return Err(ServeError::UnsafePath);
        }
        let relative = path.trim_start_matches('/');
        if std::path::Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
            || relative.contains('\\')
        {
            return Err(ServeError::UnsafePath);
        }
        let root = self.config.web.static_dir.join(domain);
        let root = fs::canonicalize(&root).map_err(|source| file_error(&root, source))?;
        let mut candidate = root.join(relative);
        if candidate.is_dir() {
            candidate.push("index.html");
        }
        let canonical = fs::canonicalize(&candidate).map_err(|source| ServeError::File {
            path: Utf8PathBuf::from_path_buf(candidate.clone())
                .unwrap_or_else(|_| self.config.web.static_dir.clone()),
            source,
        })?;
        if !canonical.starts_with(&root) {
            return Err(ServeError::UnsafePath);
        }
        Utf8PathBuf::from_path_buf(canonical).map_err(|_| ServeError::UnsafePath)
    }
}

fn file_error(path: &Utf8PathBuf, source: std::io::Error) -> ServeError {
    ServeError::File {
        path: path.clone(),
        source,
    }
}

/// A request could not be safely served.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The request escapes the selected domain root.
    #[error("resolve path inside the selected domain")]
    UnsafePath,
    /// A source file is unavailable.
    #[error("access file {path}")]
    File {
        /// File involved in the operation.
        path: Utf8PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
}
