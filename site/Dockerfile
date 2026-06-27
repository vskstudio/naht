# syntax=docker/dockerfile:1
# ---------------------------------------------------------------------------
# naht.dev — static documentation site.
#
# Two stages: build the Vite SPA with Node, then serve the static `dist/` with
# an UNPRIVILEGED nginx (runs as uid 101, binds :8080 — no root, no cap_net).
# Both base images are pinned by digest so the build is reproducible and a moved
# tag can't slip a different image underneath us.
#
# The edge reverse proxy terminates TLS and owns all security headers / origin
# lock-down; this image only serves files. Nothing here listens on a public
# port — the container is reachable only on the internal `web` Docker network.
# ---------------------------------------------------------------------------

# --- build stage -----------------------------------------------------------
FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293 AS build
WORKDIR /app

# Install deps from the lockfile only (no implicit upgrades), then build.
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY . .
RUN npm run build

# --- runtime stage ---------------------------------------------------------
FROM nginxinc/nginx-unprivileged:1.29-alpine@sha256:0c79d56aee561a1d81c63f00eee5fb5fe29279560cdc55e91425133104c7fbe6 AS runtime

# Patch OS packages to pick up security fixes published after the base image was
# built (e.g. openssl/libxml2/libexpat HIGH CVEs the digest pin would otherwise
# freeze us on). Needs root; the base image drops back to uid 101 below via USER.
USER root
RUN apk upgrade --no-cache
USER 101

# Non-root by default in this image (USER 101). Replace the stock site config.
COPY --chown=101:101 nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build --chown=101:101 /app/dist /usr/share/nginx/html

EXPOSE 8080

# Liveness for compose/orchestrator health checks (served on the internal port).
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["wget", "-qO-", "http://127.0.0.1:8080/healthz"]
