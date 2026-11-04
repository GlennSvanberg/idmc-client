//! `.env` loading. Existing process variables win, so a shell export is not
//! overwritten by the file. Values are never printed.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendMode {
    Mock,
    Live,
}

impl BackendMode {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "mock" => Ok(Self::Mock),
            "live" => Ok(Self::Live),
            other => Err(format!("backend must be mock or live (got {other})")),
        }
    }
}

/// Read the first `.env` found from the working directory upward.
pub fn load_dotenv() {
    let Ok(start) = std::env::current_dir() else { return };
    let mut dir = start;
    loop {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                apply_assignments(&parse_env_assignments(&text));
            }
            return;
        }
        if !dir.pop() {
            return;
        }
    }
}

pub fn parse_env_assignments(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        if key.is_empty() || key.contains(' ') {
            continue;
        }
        out.push((key.to_string(), unquote(value.trim())));
    }
    out
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn apply_assignments(pairs: &[(String, String)]) {
    for (key, value) in pairs {
        if std::env::var(key).is_err() {
            std::env::set_var(key, value);
        }
    }
}

pub fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

pub fn env_string(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let value = value.trim();
    if value.is_empty() { None } else { Some(value.to_string()) }
}

pub fn mode_from_env() -> Result<BackendMode, String> {
    match env_string("IDMC_BACKEND") {
        Some(value) => BackendMode::parse(&value),
        None => Ok(BackendMode::Mock),
    }
}

pub struct EnvConfig {
    pub mode: BackendMode,
    pub login_url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub model_export: Option<PathBuf>,
    pub debug_dump: Option<PathBuf>,
}

pub fn read_env() -> Result<EnvConfig, String> {
    Ok(EnvConfig {
        mode: mode_from_env()?,
        login_url: env_string("IDMC_LOGIN_URL"),
        username: env_string("IDMC_USERNAME"),
        password: env_string("IDMC_PASSWORD"),
        model_export: env_path("IDMC_MODEL_EXPORT"),
        debug_dump: env_path("IDMC_DEBUG_DUMP"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_file_keeps_comments_and_quotes() {
        let pairs = parse_env_assignments(
            "# comment\nexport IDMC_LOGIN_URL=https://dm-eu.informaticacloud.com\nIDMC_PASSWORD=\"p a\"\n\nnope\n",
        );
        assert_eq!(
            pairs,
            vec![
                ("IDMC_LOGIN_URL".into(), "https://dm-eu.informaticacloud.com".into()),
                ("IDMC_PASSWORD".into(), "p a".into()),
            ]
        );
        assert!(BackendMode::parse("LIVE").unwrap() == BackendMode::Live);
        assert!(BackendMode::parse("mock").unwrap() == BackendMode::Mock);
        assert!(BackendMode::parse("cloud").is_err());
    }
}
