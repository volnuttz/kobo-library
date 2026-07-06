FROM rust:1-alpine AS build

WORKDIR /app
RUN apk add --no-cache musl-dev

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static
RUN cargo build --release

FROM alpine:latest

WORKDIR /app
ARG KEPUBIFY_VERSION=v4.0.4

RUN arch="$(apk --print-arch)" && \
    case "$arch" in \
      x86_64) kepubify_asset="kepubify-linux-64bit" ;; \
      aarch64) kepubify_asset="kepubify-linux-arm64" ;; \
      armv7) kepubify_asset="kepubify-linux-arm" ;; \
      armhf) kepubify_asset="kepubify-linux-armv6" ;; \
      *) echo "Unsupported architecture: $arch" >&2; exit 1 ;; \
    esac && \
    wget -O /usr/local/bin/kepubify "https://github.com/pgaskin/kepubify/releases/download/${KEPUBIFY_VERSION}/${kepubify_asset}" && \
    chmod +x /usr/local/bin/kepubify

COPY --from=build /app/target/release/kobo-library /usr/local/bin/kobo-library
COPY static ./static
RUN mkdir -p data/books data/uploads

ENV DATA_DIR=/app/data
EXPOSE 3001
CMD ["kobo-library"]
