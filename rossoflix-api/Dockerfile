# Build
FROM rust:1.79 as builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src && echo "fn main(){}" > src/main.rs
RUN cargo build --release || true
COPY src ./src
RUN cargo build --release

# Runtime
FROM gcr.io/distroless/cc-debian12
ENV RUST_LOG=info
WORKDIR /app
COPY --from=builder /app/target/release/rossoflix-api /usr/local/bin/app
EXPOSE 8080
CMD ["/usr/local/bin/app"]
