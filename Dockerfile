FROM rust:1

WORKDIR /build

ENV TARGET=x86_64-unknown-linux-musl

RUN rustup target add $TARGET

COPY Cargo.toml Cargo.toml  ./

RUN mkdir src && \
    echo "fn main() {println!(\"if you see this, the build broke\")}" > src/main.rs && \
    cargo build --release --target=${TARGET} && \
    rm -rf src && \
    rm -rf target/release

COPY src ./src

RUN cargo build --release --target=${TARGET}

FROM scratch

COPY --from=0 /build/target/x86_64-unknown-linux-musl/release/hostapd-api /hostapd-api

ENTRYPOINT ["/hostapd-api"]