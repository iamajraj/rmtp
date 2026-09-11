use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpStream;

use crate::config::Config;
use crate::{parse, smtp, store, web};

pub async fn send_test() -> Result<()> {
    let to = "you@rmpt.test";
    let subject = "rmpt test email";
    let config = Config::from_env();
    let raw = build_test_message(to, subject);

    // If a server is already listening, deliver through the real SMTP path.
    if TcpStream::connect(("127.0.0.1", config.smtp_port))
        .await
        .is_ok()
    {
        smtp::send_raw("127.0.0.1", config.smtp_port, "rmpt@rmpt.test", &[to.to_string()], &raw)
            .await?;
        println!("✔ Test email sent — open http://localhost:{}", config.http_port);
        return Ok(());
    }

    // Otherwise boot the server, seed the test email, and stay alive.
    let store = Arc::new(store::Store::open(&config.db_path)?);
    let id = uuid::Uuid::new_v4().to_string();
    let parsed = parse::parse(&raw);
    store.insert(&id, "rmpt@rmpt.test", to, &parsed, &raw)?;

    let web_state = web::AppState {
        store: store.clone(),
    };
    let http_host = config.http_host.clone();
    let http_port = config.http_port;

    println!(
        "✔ Server started — test email sent — open http://localhost:{http_port} (Ctrl-C to stop)"
    );
    tokio::try_join!(
        smtp::run(config.clone(), store.clone()),
        web::run(web_state, http_host, http_port)
    )?;
    Ok(())
}

fn build_test_message(to: &str, subject: &str) -> Vec<u8> {
    let date = chrono::Utc::now().to_rfc2822();
    let esc_to = to.replace(['<', '>'], "");
    let msg = format!(
        "From: rmpt <rmpt@rmpt.test>\r\n\
         To: {esc_to}\r\n\
         Subject: {subject}\r\n\
         Date: {date}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"rmpt-mixed\"\r\n\
         \r\n\
         --rmpt-mixed\r\n\
         Content-Type: multipart/alternative; boundary=\"rmpt-alt\"\r\n\
         \r\n\
         --rmpt-alt\r\n\
         Content-Type: text/plain; charset=\"utf-8\"\r\n\
         \r\n\
         Hi,\r\n\
         \r\n\
         This is a test email sent by rmpt.\r\n\
         Your local SMTP server and inbox are working.\r\n\
         \r\n\
         — rmpt\r\n\
         --rmpt-alt\r\n\
         Content-Type: text/html; charset=\"utf-8\"\r\n\
         \r\n\
         <html><body style=\"font-family:system-ui,sans-serif;max-width:480px;margin:0 auto;padding:24px\">\r\n\
         <h2 style=\"margin-top:0\">rmpt test email</h2>\r\n\
         <p>This is a test email sent by <b>rmpt</b>.</p>\r\n\
         <p style=\"color:#666\">Your local SMTP server and inbox are working.</p>\r\n\
         </body></html>\r\n\
         --rmpt-alt--\r\n\
         --rmpt-mixed\r\n\
         Content-Type: text/plain; name=\"hello.txt\"\r\n\
         Content-Disposition: attachment; filename=\"hello.txt\"\r\n\
         \r\n\
         Hello from rmpt!\r\n\
         --rmpt-mixed--\r\n"
    );
    msg.into_bytes()
}