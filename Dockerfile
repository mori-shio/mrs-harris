# ---- Build Stage ----
FROM rust:1.95-bookworm AS builder

WORKDIR /app

# 全てのソースコードをコピー
COPY . .

# BuildKitのキャッシュマウントを使用してビルドを高速化
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin mrs-harris --bin mrs-harris-worker && \
    cp /app/target/release/mrs-harris /usr/local/bin/mrs-harris && \
    cp /app/target/release/mrs-harris-worker /usr/local/bin/mrs-harris-worker

# ---- Runtime Stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# バイナリをコピー
COPY --from=builder /usr/local/bin/mrs-harris /usr/local/bin/
COPY --from=builder /usr/local/bin/mrs-harris-worker /usr/local/bin/


# 静的ファイルをコピー
COPY --from=builder /app/static /app/static
COPY --from=builder /app/config/examples /app/config/examples

WORKDIR /app
EXPOSE 8080

# デフォルトは Controller モード
ENTRYPOINT ["mrs-harris"]
CMD ["controller", "--config", "/app/config/controller.toml"]
