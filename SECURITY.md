# Security

## Reporting a vulnerability

Please report security issues privately via GitHub's **Report a vulnerability**
(Security → Advisories) on this repository, or by email to the maintainer. Do not
open a public issue for an undisclosed vulnerability. We aim to acknowledge within
72 hours.

## Supply-chain & CI posture

CI (`.github/workflows/`) is locked down:

- **Least-privilege token** — `permissions: contents: read` at the top of every
  workflow; jobs opt into more only when strictly required (`release.yml` →
  `contents: write` for the release upload, nothing else).
- **Pinned actions** — every third-party action is pinned to a full commit SHA,
  not a moving tag, so a re-tagged release can't swap code in underneath us.
- **`persist-credentials: false`** on every checkout — the `GITHUB_TOKEN` is not
  left on disk for later steps to exfiltrate.
- **Egress auditing** — `step-security/harden-runner` runs in audit mode on each
  job to surface unexpected outbound network calls.
- **Dependency review** — PRs are gated by `dependency-review-action`
  (fail on high severity); `npm audit --omit=dev --audit-level=high` runs in CI.
- **Image scanning** — the production Docker image is built in CI and scanned
  with Trivy (fails on HIGH/CRITICAL, `ignore-unfixed`).
- **No automated deploys** — CI builds and verifies only. It never pushes an
  image to a registry and never deploys. Production is brought up by hand.

## Production deployment (naht.dev)

The site is a static-file container behind the shared `edge` nginx reverse proxy.
There is **no deploy script, no webhook, and no CI publish** — deployment is a
deliberate manual action on the host.

### Container hardening (`site/`)

- Multi-stage build; both base images **pinned by digest**.
- Runs as **non-root** (`nginx-unprivileged`, uid 101) on the unprivileged port
  `8080`.
- `read_only` root filesystem, `cap_drop: ALL`, `no-new-privileges`, tmpfs for
  the only writable paths (`/tmp`, `/var/cache/nginx`).
- **No published host port** — reachable only on the internal `web` Docker
  network, via the edge. The container is never exposed to the public internet
  directly.
- Production bundle ships **without source maps** (`vite.config.js`), so no
  original source or local filesystem paths leak.

### Edge & origin lock-down (separate `edge` repo)

The edge terminates TLS and owns all security response headers (HSTS, nosniff,
Referrer-Policy, Permissions-Policy). Origin is locked down so a leaked origin IP
cannot be hit directly:

- host firewall restricts `:443` to Cloudflare ranges (primary lock-down);
- nginx `geo` gate returns `444` to any non-Cloudflare peer (defence-in-depth);
- **Authenticated Origin Pulls (mTLS)** are enforced globally at the edge —
  the origin rejects any TLS connection not presenting Cloudflare's origin-pull
  client cert.

### Manual go-live checklist

1. Create a Cloudflare **Origin Certificate** for `naht.dev` (apex + `www` SAN)
   and place it on the host at `certs/naht/origin.{pem,key}` (key `0600`).
2. Point Cloudflare DNS: apex `naht.dev` and `www.naht.dev` → the ganyu host IP
   (proxied / orange-cloud).
3. Cloudflare **SSL/TLS → Full (strict)**.
4. Enable **Authenticated Origin Pulls** on the `naht.dev` zone (the edge enforces
   mTLS globally — without this, legitimate Cloudflare→origin traffic is rejected).
5. Bring up the site container on the host: `cd site && docker compose up -d --build`
   (joins the external `web` network as `naht_site:8080`).
6. Deploy the edge config (adds the `naht.dev` vhost): `sh deploy.sh` in the edge
   repo — it validates `nginx -t` before applying and auto-rolls-back on failure.
