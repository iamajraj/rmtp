use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub http_host: String,
    pub http_port: u16,
    pub hostname: String,
    pub db_path: String,
    pub max_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            smtp_host: "0.0.0.0".into(),
            smtp_port: 1025,
            http_host: "0.0.0.0".into(),
            http_port: 8025,
            hostname: "localhost".into(),
            db_path: String::new(), // empty = in-memory only, no files on disk
            max_message_size: 10 * 1024 * 1024,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = env::var("RMPT_SMTP_HOST") {
            cfg.smtp_host = v;
        }
        if let Ok(v) = env::var("RMPT_SMTP_PORT") {
            cfg.smtp_port = v.parse().unwrap_or(cfg.smtp_port);
        }
        if let Ok(v) = env::var("RMPT_HTTP_HOST") {
            cfg.http_host = v;
        }
        if let Ok(v) = env::var("RMPT_HTTP_PORT") {
            cfg.http_port = v.parse().unwrap_or(cfg.http_port);
        }
        if let Ok(v) = env::var("RMPT_HOSTNAME") {
            cfg.hostname = v;
        }
        if let Ok(v) = env::var("RMPT_DB_PATH") {
            cfg.db_path = v;
        }
        if let Ok(v) = env::var("RMPT_MAX_MESSAGE_SIZE") {
            cfg.max_message_size = v.parse().unwrap_or(cfg.max_message_size);
        }
        cfg
    }
}