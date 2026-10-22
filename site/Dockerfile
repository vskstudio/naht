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

FROM node:26-alpine@sha256:40cd011b55415bc894e45b02fdd5a5dfde056921b7d23a64c74f80497af48797 AS build
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

# The production image only needs the generated static files. Keep the shell and cp utilities used
# by docker-compose, but exclude the build toolchain, npm, and development dependencies from the
# scanned/runtime image.
FROM node:26-alpine@sha256:40cd011b55415bc894e45b02fdd5a5dfde056921b7d23a64c74f80497af48797 AS runtime
RUN apk upgrade --no-cache \
    && rm -rf /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/corepack
WORKDIR /app
COPY --from=build /app/dist /app/dist
