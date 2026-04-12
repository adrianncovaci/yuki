FROM gcr.io/distroless/cc-debian12
COPY target/release/yuki /yuki
ENTRYPOINT ["/yuki", "--daemon", "--config", "/config/config.toml"]
