use std::io;
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};

use crate::config::Config;
use crate::store::Store;

pub async fn run(config: Config, store: Arc<Store>) -> Result<()> {
    let addr = format!("{}:{}", config.smtp_host, config.smtp_port);
    let listener = TcpListener::bind(&addr).await?;
    info!("SMTP server listening on {addr}");

    loop {
        let (stream, peer) = listener.accept().await?;
        info!("SMTP connection from {peer}");
        let store = store.clone();
        let hostname = config.hostname.clone();
        let max_size = config.max_message_size;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, store, hostname, max_size).await {
                warn!("SMTP session with {peer} ended with error: {e:#}");
            }
        });
    }
}

async fn write_line(wr: &mut OwnedWriteHalf, line: &str) -> Result<()> {
    wr.write_all(format!("{line}\r\n").as_bytes()).await?;
    wr.flush().await?;
    Ok(())
}

/// Minimal SMTP client: delivers a raw message to a running rmpt server.
pub async fn send_raw(host: &str, port: u16, from: &str, to: &[String], raw: &[u8]) -> Result<()> {
    let stream = TcpStream::connect((host, port)).await?;
    let (rd, mut wr) = stream.into_split();
    let mut reader = BufReader::new(rd);

    read_line(&mut reader, 2048).await?; // greeting

    write_line(&mut wr, "EHLO rmpt-cli").await?;
    loop {
        let l = read_line(&mut reader, 2048)
            .await?
            .unwrap_or_default();
        if !l.starts_with("250-") {
            break;
        }
    }

    write_line(&mut wr, &format!("MAIL FROM:<{from}>")).await?;
    let _ = read_line(&mut reader, 2048).await?;
    for rcpt in to {
        write_line(&mut wr, &format!("RCPT TO:<{rcpt}>")).await?;
        let _ = read_line(&mut reader, 2048).await?;
    }

    write_line(&mut wr, "DATA").await?;
    let _ = read_line(&mut reader, 2048).await?;

    for line in raw.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.starts_with(b".") {
            wr.write_all(b".").await?;
        }
        wr.write_all(line).await?;
        wr.write_all(b"\r\n").await?;
    }
    wr.write_all(b".\r\n").await?;
    wr.flush().await?;
    let _ = read_line(&mut reader, 2048).await?;

    write_line(&mut wr, "QUIT").await?;
    let _ = read_line(&mut reader, 2048).await?;
    Ok(())
}

/// Reads a single line (up to a `\n`). Strips trailing CR/LF. Returns None on clean EOF.
async fn read_line(reader: &mut BufReader<OwnedReadHalf>, max: usize) -> Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::with_capacity(128);
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if buf.is_empty() {
                return Ok(None);
            }
            break;
        }
        match available.iter().position(|&b| b == b'\n') {
            Some(p) => {
                buf.extend_from_slice(&available[..=p]);
                reader.consume(p + 1);
                break;
            }
            None => {
                buf.extend_from_slice(available);
                let n = available.len();
                reader.consume(n);
                if buf.len() > max {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long").into());
                }
            }
        }
    }
    while buf.last() == Some(&b'\n') || buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Some(String::from_utf8_lossy(&buf).to_string()))
}

async fn handle_connection(
    stream: TcpStream,
    store: Arc<Store>,
    hostname: String,
    max_size: usize,
) -> Result<()> {
    let (rd, mut wr) = stream.into_split();
    let mut reader = BufReader::new(rd);

    write_line(&mut wr, &format!("220 {hostname} rmpt local mail capture")).await?;

    let mut mail_from: Option<String> = None;
    let mut rcpt_to: Vec<String> = Vec::new();

    loop {
        let Some(line) = read_line(&mut reader, 2048).await? else {
            break;
        };
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();
        info!("SMTP < {trimmed}");

        if upper == "QUIT" {
            write_line(&mut wr, "221 Bye").await?;
            break;
        } else if upper == "EHLO" || upper.starts_with("EHLO ") {
            write_line(&mut wr, &format!("250-{hostname} greets you")).await?;
            write_line(&mut wr, "250-8BITMIME").await?;
            write_line(&mut wr, "250-AUTH PLAIN LOGIN").await?;
            write_line(&mut wr, "250-SIZE 10485760").await?;
            write_line(&mut wr, "250 OK").await?;
        } else if upper == "HELO" || upper.starts_with("HELO ") {
            write_line(&mut wr, &format!("250 {hostname} greets you")).await?;
        } else if upper == "AUTH" || upper.starts_with("AUTH ") {
            if !handle_auth(&mut reader, &mut wr, trimmed[5..].trim()).await? {
                break;
            }
        } else if upper == "STARTTLS" {
            write_line(&mut wr, "454 TLS not available").await?;
        } else if upper.starts_with("MAIL FROM:") {
            if let Some(from) = parse_path(&trimmed[10..]) {
                mail_from = Some(from);
                rcpt_to.clear();
                write_line(&mut wr, "250 OK").await?;
            } else {
                write_line(&mut wr, "501 Syntax error in MAIL FROM").await?;
            }
        } else if upper.starts_with("RCPT TO:") {
            if let Some(to) = parse_path(&trimmed[8..]) {
                if mail_from.is_none() {
                    write_line(&mut wr, "503 No MAIL FROM specified").await?;
                } else {
                    rcpt_to.push(to);
                    write_line(&mut wr, "250 OK").await?;
                }
            } else {
                write_line(&mut wr, "501 Syntax error in RCPT TO").await?;
            }
        } else if upper == "DATA" {
            if mail_from.is_none() || rcpt_to.is_empty() {
                write_line(&mut wr, "503 Bad sequence of commands").await?;
                continue;
            }
            write_line(&mut wr, "354 End with <CRLF>.<CRLF>").await?;

            let mut data: Vec<u8> = Vec::new();
            let mut too_big = false;
            loop {
                match read_line(&mut reader, 2048).await? {
                    None => break,
                    Some(l) => {
                        if l == "." {
                            break;
                        }
                        let bytes: &[u8] = l.as_bytes();
                        let bytes = bytes.strip_prefix(b".").unwrap_or(bytes);
                        data.extend_from_slice(bytes);
                        data.push(b'\n');
                        if data.len() > max_size {
                            too_big = true;
                        }
                    }
                }
            }

            if too_big {
                write_line(&mut wr, "552 Message size exceeds fixed maximum message size").await?;
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let parsed = crate::parse::parse(&data);
                let from = mail_from.clone().unwrap_or_default();
                store.insert(&id, &from, &rcpt_to.join(", "), &parsed, &data)?;
                info!(
                    "Captured message {id}: from={from} to={} subject={:?} size={}",
                    rcpt_to.join(", "),
                    parsed.subject,
                    data.len()
                );
                write_line(&mut wr, &format!("250 OK: queued as {id}")).await?;
            }
            mail_from = None;
            rcpt_to.clear();
        } else if upper == "RSET" {
            mail_from = None;
            rcpt_to.clear();
            write_line(&mut wr, "250 OK").await?;
        } else if upper == "NOOP" {
            write_line(&mut wr, "250 OK").await?;
        } else if upper.starts_with("VRFY") {
            write_line(&mut wr, "252 Cannot VRFY user, but will accept message").await?;
        } else {
            write_line(&mut wr, "500 Command unrecognized").await?;
        }
    }

    Ok(())
}

fn parse_path(s: &str) -> Option<String> {
    let s = s.trim();
    // Drop ESMTP parameters that follow the closing ">" (e.g. "SIZE=123", "BODY=8BITMIME")
    let s = match s.find('>') {
        Some(end) => &s[..=end],
        None => s,
    };
    let s = s
        .strip_prefix('<')
        .and_then(|x| x.strip_suffix('>'))
        .unwrap_or(s);
    let s = s.trim();
    if s.contains('@') || s.is_empty() {
        Some(s.to_string())
    } else {
        None
    }
}

/// Accepts any AUTH mechanism (PLAIN / LOGIN) with any credentials.
/// Returns Ok(false) if the connection should be closed.
async fn handle_auth(
    reader: &mut BufReader<OwnedReadHalf>,
    wr: &mut OwnedWriteHalf,
    args: &str,
) -> Result<bool> {
    let (mech, inline) = match args.split_once(' ') {
        Some((m, rest)) => (m, rest.trim()),
        None => (args, ""),
    };

    let accepted = match mech.to_uppercase().as_str() {
        "PLAIN" => {
            if inline.is_empty() {
                write_line(wr, "334 ").await?;
                if read_line(reader, 2048).await?.is_none() {
                    return Ok(false);
                }
            }
            true
        }
        "LOGIN" => {
            if inline.is_empty() {
                write_line(wr, "334 ").await?;
                if read_line(reader, 2048).await?.is_none() {
                    return Ok(false);
                }
            }
            write_line(wr, "334 ").await?;
            if read_line(reader, 2048).await?.is_none() {
                return Ok(false);
            }
            true
        }
        _ => {
            write_line(wr, "504 Unrecognized authentication type").await?;
            false
        }
    };

    if accepted {
        write_line(wr, "235 2.7.0 Authentication successful").await?;
        info!("SMTP auth accepted (any credentials)");
    }
    Ok(true)
}