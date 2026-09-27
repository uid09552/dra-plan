# syntax=docker/dockerfile:1.7
# dra-workflow UI: build stage (Angular) → run stage (nginx, non-root, port 8080).
#   docker build -t dra-ui frontend/
#   docker run -p 8080:8080 -e BACKEND_URL=http://backend:8090 dra-ui
# nginx serves the SPA and proxies /api and /mcp to BACKEND_URL (same origin, no CORS needed).

ARG NODE_VERSION=24
ARG NGINX_VERSION=1.29

# ── Build ────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION}-alpine AS build
WORKDIR /src
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci --no-audit --no-fund
COPY . .
RUN npx ng build --configuration production

# ── Run ──────────────────────────────────────────────────────────────────
FROM nginxinc/nginx-unprivileged:${NGINX_VERSION}-alpine AS run
# Rendered to /etc/nginx/conf.d/default.conf by the image's envsubst entrypoint at start-up.
COPY nginx/default.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /src/dist/frontend/browser /usr/share/nginx/html

ENV BACKEND_URL=http://backend:8090
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --retries=3 \
    CMD wget -q -O /dev/null http://127.0.0.1:8080/healthz || exit 1
