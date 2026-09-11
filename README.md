# rmtp — local SMTP server + inbox

A developer tool that runs a local SMTP server and captures every email your
application sends, then shows it in a clean local web inbox. Perfect for
testing OTP / verification flows when you don't have real SMTP access.

No real mail is ever sent. No configuration required.

```
cargo run
```

### Quick test — no code needed

Send a test email instantly with a single command (no arguments required):

```bash
cargo run -- test
```

If the server is already running it delivers over SMTP; if not, it starts the
server and seeds one test email, then stays alive.

| Port | Purpose |
| --- | --- |
| `1025` | SMTP server (what your app connects to) |
| `8025` | Web inbox (http://localhost:8025) |

## How it works

1. Your app sends mail through `localhost:1025` using its normal SMTP config.
2. rmtp accepts it — **any** username/password, **any** from/to address.
3. The email lands in the local inbox at `http://localhost:8025`.

## Using it from your app

Set the SMTP host/port and use any username/password (they're ignored):

- **Host**: `localhost` (or your LAN IP when the app is on another machine)
- **Port**: `1025`
- **Username / Password**: anything, e.g. `rmtp` / `rmtp`
- **From**: anything

### Symfony (PHP)

```dotenv
MAILER_DSN=smtp://rmtp:rmtp@localhost:1025
```

### Node (nodemailer)

```js
const transporter = nodemailer.createTransport({
  host: 'localhost',
  port: 1025,
  secure: false,
  auth: { user: 'rmtp', pass: 'rmtp' },
});
```

### Python

```python
import smtplib
s = smtplib.SMTP('localhost', 1025)
s.login('rmtp', 'rmtp')   # optional
s.send_message(msg)
```

### Rails (Action Mailer)

```ruby
config.action_mailer.smtp_settings = {
  address: 'localhost', port: 1025,
  user_name: 'rmtp', password: 'rmtp',
  authentication: :plain,
}
```

## What it supports

- **SMTP**: `EHLO/HELO`, `MAIL FROM`, `RCPT TO`, `DATA`, `RSET`, `NOOP`, `QUIT`, `VRFY`
- **Auth**: `AUTH PLAIN` / `AUTH LOGIN` — accepts any credentials
- **ESMTP**: `SIZE` / `BODY=8BITMIME` parameters on `MAIL FROM`
- **MIME**: HTML + plain-text bodies, attachments, headers, nested messages
- **Inbox**: search, per-message preview / text / headers / raw views, attachment download, delete

## Configuration (optional)

Everything works with defaults. All settings are overridable via environment
variables:

| Variable | Default | Description |
| --- | --- | --- |
| `RMTP_SMTP_HOST` | `0.0.0.0` | SMTP bind address |
| `RMTP_SMTP_PORT` | `1025` | SMTP port |
| `RMTP_HTTP_HOST` | `0.0.0.0` | Web inbox bind address |
| `RMTP_HTTP_PORT` | `8025` | Web inbox port |
| `RMTP_HOSTNAME` | `localhost` | Hostname advertised in the SMTP greeting |
| `RMTP_DB_PATH` | *(empty)* | SQLite database file. Empty = in-memory only (nothing written to disk) |
| `RMTP_MAX_MESSAGE_SIZE` | `10485760` | Max message size in bytes |

By default messages are kept **in memory only** — no files are created. Set
`RMTP_DB_PATH` to a path to persist mail across restarts:

```bash
RMTP_DB_PATH=~/.rmtp.db cargo run
```

Example:

```bash
RMTP_SMTP_PORT=2525 RMTP_HTTP_PORT=8026 RMTP_DB_PATH=~/.rmtp.db cargo run
```

## HTTP API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/` | Web inbox |
| `GET` | `/api/health` | Status + message count |
| `GET` | `/api/messages?q=&page=&limit=` | List/search messages |
| `GET` | `/api/messages/{id}` | Full message (bodies, headers, attachments meta) |
| `GET` | `/api/messages/{id}/raw` | Raw RFC 5322 message |
| `GET` | `/api/messages/{id}/attachments/{attId}` | Download attachment |
| `DELETE` | `/api/messages/{id}` | Delete one message |
| `DELETE` | `/api/messages` | Clear the inbox |

## Development

```
cargo run            # start the server
cargo build          # compile (also embeds the web UI)
```

The web UI lives in `src/ui/index.html` and is embedded into the binary at
compile time, so UI changes require a rebuild.

## Project layout

```
src/
  main.rs      # entry point: `rmtp` = server, `rmtp test` = test email
  cli.rs       # test-email sender (boots server or uses the running one)
  config.rs    # environment-based configuration
  smtp.rs      # SMTP protocol server + minimal client (tokio)
  parse.rs     # MIME parsing (mail-parser)
  store.rs     # SQLite persistence
  web.rs       # Axum web server + JSON API
  ui/          # embedded inbox frontend
```