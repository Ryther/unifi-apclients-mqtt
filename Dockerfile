FROM rust:1-bookworm@sha256:93ce27a88655056a51dbdd8f5f2d7ddc071c7b0070fb288a37b5a285fc83971e AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
RUN apt-get update \
    && apt-get install --yes --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add x86_64-unknown-linux-musl
RUN CC_x86_64_unknown_linux_musl=musl-gcc \
    cargo build --release --locked --target x86_64-unknown-linux-musl
RUN install -d -m 0700 /runtime/data /runtime/tmp

FROM scratch
COPY --from=builder --chown=10001:10001 --chmod=0700 /runtime/data /data
COPY --from=builder --chown=10001:10001 --chmod=0700 /runtime/tmp /tmp
COPY --from=builder /src/target/x86_64-unknown-linux-musl/release/unifi-apclients-mqtt /usr/local/bin/unifi-apclients-mqtt
ENV CLIENT_HISTORY_DB=/data/client-history.db
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/unifi-apclients-mqtt"]
