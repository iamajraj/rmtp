use chrono::{FixedOffset, NaiveDate, NaiveTime, TimeZone};
use mail_parser::{Address, ContentType, HeaderValue, MessageParser, MimeHeaders, PartType};

use crate::store::Header;

#[derive(Debug, Clone)]
pub struct ParsedAttachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Parsed {
    pub from_name: Option<String>,
    pub from_addr: Option<String>,
    pub to_addr: Option<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub date: Option<String>,
    pub body_plain: Option<String>,
    pub body_html: Option<String>,
    pub headers: Vec<Header>,
    pub attachments: Vec<ParsedAttachment>,
}

impl Default for Parsed {
    fn default() -> Self {
        Self {
            from_name: None,
            from_addr: None,
            to_addr: None,
            cc: Vec::new(),
            subject: String::new(),
            date: None,
            body_plain: None,
            body_html: None,
            headers: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

pub fn parse(raw: &[u8]) -> Parsed {
    let mut parsed = Parsed::default();
    let Some(msg) = MessageParser::default().parse(raw) else {
        return parsed;
    };

    parsed.subject = msg.subject().unwrap_or("").to_string();

    if let Some(from) = msg.from().and_then(|a| a.first()) {
        parsed.from_name = from.name.as_deref().map(str::to_string);
        parsed.from_addr = from.address.as_deref().map(str::to_string);
    }
    let tos = collect_addresses(msg.to());
    parsed.to_addr = if tos.is_empty() {
        None
    } else {
        Some(tos.join(", "))
    };
    parsed.cc = collect_addresses(msg.cc());

    parsed.date = msg.date().map(format_datetime);

    for h in msg.headers() {
        parsed.headers.push(Header {
            name: h.name().to_string(),
            value: header_value_to_string(&h.value),
        });
    }

    if let Some(t) = msg.body_text(0) {
        parsed.body_plain = Some(t.into_owned());
    }
    if let Some(h) = msg.body_html(0) {
        parsed.body_html = Some(h.into_owned());
    }

    for att in msg.attachments() {
        let data = match &att.body {
            PartType::Binary(b) | PartType::InlineBinary(b) => b.to_vec(),
            PartType::Text(t) => t.as_bytes().to_vec(),
            PartType::Html(h) => h.as_bytes().to_vec(),
            PartType::Message(m) => m.raw_message().to_vec(),
            PartType::Multipart(_) => continue,
        };
        let ct = att
            .content_type()
            .map(format_content_type)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        parsed.attachments.push(ParsedAttachment {
            filename: att.attachment_name().unwrap_or("attachment").to_string(),
            content_type: ct,
            data,
        });
    }

    parsed
}

fn collect_addresses(address: Option<&Address>) -> Vec<String> {
    let Some(addr) = address else {
        return Vec::new();
    };
    addr.iter()
        .map(|a| match (a.name.as_deref(), a.address.as_deref()) {
            (Some(n), Some(e)) => format!("{n} <{e}>"),
            (None, Some(e)) => e.to_string(),
            (Some(n), None) => n.to_string(),
            (None, None) => String::new(),
        })
        .collect()
}

fn header_value_to_string(h: &HeaderValue) -> String {
    match h {
        HeaderValue::Text(t) => t.to_string(),
        HeaderValue::TextList(l) => l.iter().map(|c| c.as_ref()).collect::<Vec<_>>().join(", "),
        HeaderValue::Address(a) => collect_addresses(Some(a)).join(", "),
        HeaderValue::DateTime(d) => format_datetime(d),
        HeaderValue::ContentType(ct) => format_content_type(ct),
        HeaderValue::Received(r) => format!("{r:?}"),
        HeaderValue::Empty => String::new(),
    }
}

fn format_content_type(ct: &ContentType) -> String {
    let mut s = ct.ctype().to_string();
    if let Some(sub) = ct.subtype() {
        s.push('/');
        s.push_str(sub);
    }
    if let Some(atts) = ct.attributes() {
        for a in atts {
            s.push_str(&format!("; {}={}", a.name, a.value));
        }
    }
    s
}

fn format_datetime(d: &mail_parser::DateTime) -> String {
    let fallback = format!(
        "{}-{:02}-{:02} {:02}:{:02}:{:02}",
        d.year, d.month, d.day, d.hour, d.minute, d.second
    );
    let Some(naive) = NaiveDate::from_ymd_opt(d.year as i32, d.month as u32, d.day as u32).and_then(
        |date| {
            NaiveTime::from_hms_opt(d.hour as u32, d.minute as u32, d.second as u32)
                .map(|time| date.and_time(time))
        },
    ) else {
        return fallback;
    };
    let offset = (d.tz_hour as i32) * 3600 + (d.tz_minute as i32) * 60;
    let offset = if d.tz_before_gmt { -offset } else { offset };
    match FixedOffset::east_opt(offset).and_then(|tz| tz.from_local_datetime(&naive).single()) {
        Some(dt) => dt.to_rfc3339(),
        None => fallback,
    }
}