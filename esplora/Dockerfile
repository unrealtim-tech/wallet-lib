FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends clang cmake git && rm -rf /var/lib/apt/lists/*
RUN git clone --depth 1 --branch new-index https://github.com/Blockstream/electrs /electrs
WORKDIR /electrs
RUN cargo build --release --bin electrs

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /electrs/target/release/electrs /usr/local/bin/electrs
ENTRYPOINT ["electrs"]
