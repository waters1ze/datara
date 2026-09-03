# Multi-stage Dockerfile for Datara & Forgen Compiler
# Publishes to GitHub Packages (GHCR: ghcr.io/<owner>/datara)

FROM rust:latest AS builder

WORKDIR /build
COPY . .

# Build release compiler
RUN cargo build --release

# Final runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    libc6-dev \
    libpthread-stubs0-dev \
    ca-certificates \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

# Install compiler binary and stdlib
COPY --from=builder /build/target/release/forgen /usr/local/bin/forgen
RUN ln -s /usr/local/bin/forgen /usr/local/bin/datara
COPY --from=builder /build/stdlib /usr/local/share/datara/stdlib

ENV DATARA_HOME=/usr/local/share/datara
ENV PATH="/usr/local/bin:${PATH}"

# Verify installation
RUN forgen --help

ENTRYPOINT ["forgen"]
CMD ["--help"]
