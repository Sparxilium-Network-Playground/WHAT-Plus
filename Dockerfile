# One Dockerfile, two images:
#   docker build --target server -t whatplus-server .
#   docker build --target agent  -t whatplus-agent  .
# Both run as uid 10001 and are Linux images (Docker Desktop on Windows runs them too).

# ---- build ----
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p whatplus-server -p whatplus-agent \
 && mkdir -p /out \
 && cp target/release/whatplus-server target/release/whatplus-agent /out/

# ---- shared runtime base ----
FROM debian:bookworm-slim AS runtime
RUN groupadd --system --gid 10001 whatplus \
 && useradd --system --uid 10001 --gid whatplus --home-dir /home/whatplus --create-home --shell /usr/sbin/nologin whatplus \
 && mkdir -p /data /config /keys /run/secrets \
 && chown whatplus:whatplus /data && chmod 700 /data
ENV LANG=C.UTF-8 TERM=xterm-256color

# ---- server ----
FROM runtime AS server
COPY --from=build /out/whatplus-server /usr/local/bin/whatplus-server
ENV WHATPLUS_CONFIG=/config/config.conf
USER whatplus
WORKDIR /data
EXPOSE 33445
ENTRYPOINT ["whatplus-server"]
CMD ["run"]

# ---- agent ----
FROM runtime AS agent
COPY --from=build /out/whatplus-agent /usr/local/bin/whatplus-agent
# 使用者的資料目錄以唯讀方式掛載到這個 HOME 底下（見 deploy/agent/docker-compose.yml）。
ENV HOME=/home/whatplus \
    CLAUDE_CONFIG_DIR=/home/whatplus/.claude \
    CODEX_HOME=/home/whatplus/.codex \
    WHATPLUS_AGENT_DATA=/data \
    WHATPLUS_AGENT_KEYS=/keys
USER whatplus
WORKDIR /data
ENTRYPOINT ["whatplus-agent"]
CMD ["run"]
