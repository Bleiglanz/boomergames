# Boomergames Production Deployment Runbook (GCP `europe-west3`)

Architecture per `incubation/plan.md` §9.7: **single container instance** holding live lobbies, tables, and running games in memory, backed by **Cloud SQL for PostgreSQL** over private IP, fronted by **Cloudflare** (free plan).

## Required Environment Variables (Secret Manager / VM unit)

| Variable | Description |
|---|---|
| `BOOMER_BIND` | Bind address (default `0.0.0.0:8080`) |
| `BOOMER_STATIC_DIR` | Compiled SPA directory (`/app/web/dist`) |
| `BOOMER_PUBLIC_ORIGIN` | Public HTTPS origin for magic links (`https://boomergames.com`) |
| `BOOMER_SESSION_SECRET` | 32+ byte random HMAC-SHA256 signing secret (from Secret Manager) |
| `DATABASE_URL` | Private-IP PostgreSQL URI (`postgres://boomer:***@10.x.x.x/boomergames`) |
| `SMTP_HOST`, `SMTP_PORT=587`, `SMTP_USER`, `SMTP_PASS`, `SMTP_FROM` | EU SMTP provider (Brevo / Mailjet) on port 587 |
| `GOOGLE_CLIENT_ID` | Optional Google Identity Services OAuth Client ID |
| `BOOMER_LOG_FORMAT=json` | Structured JSON logs for Google Cloud Logging |

## Deployment Flow

1. **Migrations:** Embedded `migrations/0001_initial.sql` runs automatically via `sqlx::migrate!` at server boot.
2. **Graceful Drain:** `docker stop --time=900 boomergames` sends `SIGTERM`. `boomer-server` broadcasts `"Server update in a few minutes — running games will finish"`, refuses new matches, and waits up to 15 minutes until `games == 0` before exiting cleanly.
3. **Health & Uptime Monitoring:** Configure a Cloud Monitoring HTTPS uptime check against `GET /status` (`200 OK` healthy / `503 Service Unavailable` degraded).
4. **Network Security:** Firewall allows TLS ingress only from Cloudflare IP ranges; SSH is strictly via Identity-Aware Proxy (`--tunnel-through-iap`); Cloud SQL has private IP only.
