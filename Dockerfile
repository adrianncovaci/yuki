FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --chmod=755 target/release/yuki /usr/local/bin/yuki

ENTRYPOINT ["yuki", "--daemon", "--config", "/config/config.toml"]
