# Build stage
FROM rust:1.88-slim-bookworm AS builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/inception-registry /usr/local/bin/

# Copy migrations
COPY --from=builder /app/migrations ./migrations

# Create data directory
RUN mkdir -p /data

# Expose ports
EXPOSE 18080 19090

# Environment variables
ENV INCEPTION_HOST=0.0.0.0
ENV INCEPTION_PORT=18080
ENV INCEPTION_DATABASE_URL=sqlite:///data/inception.db
ENV INCEPTION_METRICS_ENABLED=true
ENV INCEPTION_METRICS_PORT=19090

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:18080/health || exit 1

# Run the binary
CMD ["inception-registry"]
