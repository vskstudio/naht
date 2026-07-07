# syntax=docker/dockerfile:1
# ---------------------------------------------------------------------------
# naht.dev — static documentation site (build-only image).
#
# There is NO runtime server in this image anymore. The edge reverse proxy
# (separate repo, EDGE_SITE=white) serves the built `dist/` straight from disk
# — no per-site nginx, no long-lived container, zero idle RAM. This image only
# produces the artifacts; the compose service copies them onto the host and
# exits (see docker-compose.yml).
#
# Node base pinned by digest so the build is reproducible and a moved tag can't
# slip a different image underneath us.
# ---------------------------------------------------------------------------

FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293 AS build
WORKDIR /app

# Install deps from the lockfile only (no implicit upgrades), then build.
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY . .
RUN npm run build

# Pre-compress every text asset at build time so the edge can serve the .gz
# with `gzip_static on` and never spend a request-time CPU cycle re-gzipping.
# -k keeps the original (for clients that don't send Accept-Encoding: gzip).
RUN find dist -type f \
      \( -name '*.html' -o -name '*.css' -o -name '*.js' -o -name '*.mjs' \
         -o -name '*.json' -o -name '*.svg' -o -name '*.xml' -o -name '*.txt' \) \
      -exec gzip -9 -k {} +
