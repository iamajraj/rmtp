mod cli;
mod config;
mod parse;
mod smtp;
mod store;
mod web;

use std::sync::Arc;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rmtp=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("test") | Some("send") | Some("test:mail") | Some("test-mail") => cli::send_test().await,
        _ => run_server().await,
    }
}

async fn run_server() -> Result<()> {
    let config = config::Config::from_env();
    let store = Arc::new(store::Store::open(&config.db_path)?);
    let db_desc = if config.db_path.is_empty() {
        "in-memory".to_string()
    } else {
        config.db_path.clone()
    };
    tracing::info!(
        "rmtp started: smtp={}:{} http=http://{}:{} db={}",
        config.smtp_host,
        config.smtp_port,
        config.http_host,
        config.http_port,
        db_desc
    );

    let web_state = web::AppState {
        store: store.clone(),
    };
    let http_host = config.http_host.clone();
    let http_port = config.http_port;

    tokio::try_join!(
        smtp::run(config.clone(), store.clone()),
        web::run(web_state, http_host, http_port)
    )?;

    Ok(())
}