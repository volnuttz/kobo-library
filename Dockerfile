FROM rust:1.85-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
COPY static ./static
RUN cargo build --release --locked

FROM debian:bookworm-slim AS kepubify

ARG TARGETARCH
ARG KEPUBIFY_VERSION=v4.0.4
# SHA-256 of kepubify-linux-64bit from the v4.0.4 release. An ARM build must
# provide the matching KEPUBIFY_SHA256 build argument explicitly.
ARG KEPUBIFY_SHA256=37d7628d26c5c906f607f24b36f781f306075e7073a6fe7820a751bb60431fc5

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && case "${TARGETARCH}" in \
        amd64) asset="kepubify-linux-64bit" ;; \
        arm64) asset="kepubify-linux-arm64" ;; \
        *) echo "Unsupported kepubify architecture: ${TARGETARCH}" >&2; exit 1 ;; \
       esac \
    && curl --fail --location --retry 3 \
        "https://github.com/pgaskin/kepubify/releases/download/${KEPUBIFY_VERSION}/${asset}" \
        --output /usr/local/bin/kepubify \
    && echo "${KEPUBIFY_SHA256}  /usr/local/bin/kepubify" | sha256sum --check \
    && chmod 0555 /usr/local/bin/kepubify \
    && /usr/local/bin/kepubify --version

FROM debian:bookworm-slim

RUN useradd --system --uid 10001 --create-home epubdrop \
    && mkdir /data \
    && chown epubdrop:epubdrop /data

COPY --from=builder /src/target/release/epub-drop /usr/local/bin/epub-drop
COPY --from=kepubify /usr/local/bin/kepubify /usr/local/bin/kepubify

ENV PORT=3001 \
    DATA_DIR=/data \
    KEPUBIFY_BIN=/usr/local/bin/kepubify

EXPOSE 3001
USER epubdrop
CMD ["/usr/local/bin/epub-drop"]
