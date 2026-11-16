# syntax=docker/dockerfile:1

# --- Frontend: Angular production build ---
FROM node:24.21.0-trixie-slim AS frontend
WORKDIR /src
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npx ng build

# --- Backend: release binary ---
FROM rust:1.99.1-trixie AS backend
WORKDIR /src
COPY backend/Cargo.toml backend/Cargo.lock ./
# Build the dependencies once with a placeholder, so code changes do not rebuild them.
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY backend/src ./src
RUN touch src/main.rs && cargo build --release --locked
# Empty data directory owned by the runtime user (distroless has no shell to create it).
RUN mkdir -p /out/data

# --- Runtime: no shell, no package manager, non-root ---
FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=backend /src/target/release/clockopus /app/clockopus
COPY --from=frontend /src/dist/clockopus/browser /app/static
COPY --from=backend --chown=65532:65532 /out/data /data
ENV CLOCKOPUS_STATIC_DIR=/app/static \
    CLOCKOPUS_DATA_DIR=/data
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/app/clockopus"]
