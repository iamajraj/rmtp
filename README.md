# rmpt — local SMTP server + inbox

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
2. rmpt accepts it — **any** username/password, **any** from/to address.
3. The email lands in the local inbox at `http://localhost:8025`.

## Using it from your app

Set the SMTP host/port and use any username/password (they're ignored):

- **Host**: `localhost` (or your LAN IP when the app is on another machine)
- **Port**: `1025`
- **Username / Password**: anything, e.g. `rmpt` / `rmpt`
- **From**: anything

### Symfony (PHP)

```dotenv
MAILER_DSN=smtp://rmpt:rmpt@localhost:1025
```

### Node (nodemailer)

```js
const transporter = nodemailer.createTransport({
  host: 'localhost',
  port: 1025,
  secure: false,
  auth: { user: 'rmpt', pass: 'rmpt' },
});
```

### Python

```python
import smtplib
s = smtplib.SMTP('localhost', 1025)
s.login('rmpt', 'rmpt')   # optional
s.send_message(msg)
```

### Rails (Action Mailer)

```ruby
config.action_mailer.smtp_settings = {
  address: 'localhost', port: 1025,
  user_name: 'rmpt', password: 'rmpt',
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
| `RMPT_SMTP_HOST` | `0.0.0.0` | SMTP bind address |
| `RMPT_SMTP_PORT` | `1025` | SMTP port |
| `RMPT_HTTP_HOST` | `0.0.0.0` | Web inbox bind address |
| `RMPT_HTTP_PORT` | `8025` | Web inbox port |
| `RMPT_HOSTNAME` | `localhost` | Hostname advertised in the SMTP greeting |
| `RMPT_DB_PATH` | `rmpt.db` | SQLite database file |
| `RMPT_MAX_MESSAGE_SIZE` | `10485760` | Max message size in bytes |

Example:

```bash
RMPT_SMTP_PORT=2525 RMPT_HTTP_PORT=8026 RMPT_DB_PATH=~/.rmpt.db cargo run
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
  main.rs      # entry point: `rmpt` = server, `rmpt test` = test email
  cli.rs       # test-email sender (boots server or uses the running one)
  config.rs    # environment-based configuration
  smtp.rs      # SMTP protocol server + minimal client (tokio)
  parse.rs     # MIME parsing (mail-parser)
  store.rs     # SQLite persistence
  web.rs       # Axum web server + JSON API
  ui/          # embedded inbox frontend
```