FROM node:22-bookworm-slim AS web-build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY tokens.css ./tokens.css
COPY web ./web
COPY scripts ./scripts
RUN npm run build

FROM rust:1.88-bookworm AS server-build
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY server ./server
RUN cargo build --locked --release -p ani-desk-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gosu \
    && rm -rf /var/lib/apt/lists/* \
    && printf 'precedence ::ffff:0:0/96  100\n' >> /etc/gai.conf \
    && groupadd --system ani-desk \
    && useradd --system --gid ani-desk --home-dir /app --shell /usr/sbin/nologin ani-desk \
    && mkdir -p /data \
    && chown ani-desk:ani-desk /data
WORKDIR /app
COPY --from=server-build /app/target/release/ani-desk-server /usr/local/bin/ani-desk-server
COPY --from=web-build /app/web/dist ./web/dist
COPY scripts/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENV ANI_DESK_DATA_DIR=/data
ENV ANI_DESK_WEB_DIR=/app/web/dist
EXPOSE 3000
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["ani-desk-server"]
