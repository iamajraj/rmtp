use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageSummary {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub received_at: i64,
    pub size: usize,
    pub has_attachments: bool,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttachmentMeta {
    pub id: i64,
    pub filename: String,
    pub content_type: String,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attachment {
    pub id: i64,
    pub filename: String,
    pub content_type: String,
    pub size: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    #[serde(flatten)]
    pub summary: MessageSummary,
    pub from_name: Option<String>,
    pub cc: Vec<String>,
    pub headers: Vec<Header>,
    pub body_plain: Option<String>,
    pub body_html: Option<String>,
    pub raw: String,
    pub attachments: Vec<AttachmentMeta>,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let conn = if path.is_empty() {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        if !path.is_empty() {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS messages (
                id              TEXT PRIMARY KEY,
                envelope_from   TEXT NOT NULL,
                envelope_to     TEXT NOT NULL,
                from_name       TEXT,
                from_addr       TEXT,
                to_addr         TEXT,
                cc              TEXT,
                subject         TEXT NOT NULL DEFAULT '',
                date            TEXT,
                body_plain      TEXT,
                body_html       TEXT,
                headers         TEXT NOT NULL DEFAULT '[]',
                raw             TEXT NOT NULL DEFAULT '',
                size            INTEGER NOT NULL DEFAULT 0,
                received_at     INTEGER NOT NULL,
                has_attachments INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS attachments (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
                filename     TEXT,
                content_type TEXT,
                size         INTEGER NOT NULL DEFAULT 0,
                data         BLOB
            );
            CREATE INDEX IF NOT EXISTS idx_messages_received_at ON messages(received_at DESC);
            ",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn insert(
        &self,
        id: &str,
        envelope_from: &str,
        envelope_to: &str,
        parsed: &crate::parse::Parsed,
        raw: &[u8],
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (
                id, envelope_from, envelope_to, from_name, from_addr, to_addr, cc,
                subject, date, body_plain, body_html, headers, raw, size, received_at, has_attachments
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            rusqlite::params![
                id,
                envelope_from,
                envelope_to,
                parsed.from_name,
                parsed.from_addr,
                parsed.to_addr,
                serde_json::to_string(&parsed.cc)?,
                parsed.subject,
                parsed.date,
                parsed.body_plain,
                parsed.body_html,
                serde_json::to_string(&parsed.headers)?,
                String::from_utf8_lossy(raw),
                raw.len() as i64,
                chrono::Utc::now().timestamp_millis(),
                parsed.attachments.len() as i64,
            ],
        )?;
        for att in &parsed.attachments {
            conn.execute(
                "INSERT INTO attachments (message_id, filename, content_type, size, data)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    id,
                    att.filename,
                    att.content_type,
                    att.data.len() as i64,
                    att.data,
                ],
            )?;
        }
        Ok(())
    }

    pub fn list(&self, search: &str, limit: usize, offset: usize) -> Result<Vec<MessageSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, from_addr, to_addr, subject, received_at, size, has_attachments
             FROM messages WHERE 1=1",
        );
        let mut params: Vec<String> = Vec::new();
        if !search.is_empty() {
            sql.push_str(" AND (subject LIKE ? OR from_addr LIKE ? OR to_addr LIKE ? OR body_plain LIKE ?)");
            let like = format!("%{}%", search);
            for _ in 0..4 {
                params.push(like.clone());
            }
        }
        sql.push_str(" ORDER BY received_at DESC LIMIT ? OFFSET ?");
        params.push(limit.to_string());
        params.push(offset.to_string());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok(MessageSummary {
                id: r.get(0)?,
                from: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                to: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                subject: r.get(3)?,
                received_at: r.get(4)?,
                size: r.get::<_, i64>(5)? as usize,
                has_attachments: r.get::<_, i64>(6)? != 0,
                snippet: String::new(),
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, from_addr, to_addr, subject, received_at, size, has_attachments,
                    from_name, cc, headers, body_plain, body_html, raw
             FROM messages WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], |r| {
            Ok((
                MessageSummary {
                    id: r.get(0)?,
                    from: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    to: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    subject: r.get(3)?,
                    received_at: r.get(4)?,
                    size: r.get::<_, i64>(5)? as usize,
                    has_attachments: r.get::<_, i64>(6)? != 0,
                    snippet: String::new(),
                },
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, String>(12)?,
            ))
        })?;

        let Some(row) = rows.next() else {
            return Ok(None);
        };
        let (summary, from_name, cc_json, headers_json, body_plain, body_html, raw) = row?;

        let cc: Vec<String> = cc_json
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default();
        let headers: Vec<Header> = headers_json
            .and_then(|h| serde_json::from_str(&h).ok())
            .unwrap_or_default();

        let attachments = {
            let mut stmt =
                conn.prepare("SELECT id, filename, content_type, size FROM attachments WHERE message_id = ?1 ORDER BY id")?;
            let mut rows = stmt.query_map([id], |r| {
                Ok(AttachmentMeta {
                    id: r.get(0)?,
                    filename: r.get(1)?,
                    content_type: r.get(2)?,
                    size: r.get::<_, i64>(3)? as usize,
                })
            })?;
            let mut atts = Vec::new();
            while let Some(a) = rows.next() {
                atts.push(a?);
            }
            atts
        };

        Ok(Some(Message {
            summary,
            from_name,
            cc,
            headers,
            body_plain,
            body_html,
            raw,
            attachments,
        }))
    }

    pub fn get_attachment(&self, message_id: &str, att_id: i64) -> Result<Option<Attachment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, filename, content_type, size, data FROM attachments
             WHERE message_id = ?1 AND id = ?2",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![message_id, att_id], |r| {
            Ok(Attachment {
                id: r.get(0)?,
                filename: r.get(1)?,
                content_type: r.get(2)?,
                size: r.get::<_, i64>(3)? as usize,
                data: r.get(4)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM messages WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn delete_all(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM attachments", [])?;
        conn.execute("DELETE FROM messages", [])?;
        Ok(())
    }
}