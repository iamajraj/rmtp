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
                .unwrap_or_else(|_| "rmpt=info".into()),
        )
        .init();

    let config = config::Config::from_env();
    let store = Arc::new(store::Store::open(&config.db_path)?);
    tracing::info!(
        "rmpt started: smtp={}:{} http=http://{}:{} db={}",
        config.smtp_host,
        config.smtp_port,
        config.http_host,
        config.http_port,
        config.db_path
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