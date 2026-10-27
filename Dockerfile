# syntax=docker/dockerfile:1

FROM rust:1.99-slim-trixie AS rust

FROM node:24-trixie-slim AS client
ARG VOSK_MODELS=true
ARG WASM_PACK_VERSION=0.15.0
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl gcc libc6-dev unzip \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /usr/local/cargo /usr/local/cargo
RUN rustup target add wasm32-unknown-unknown \
    && curl -fsSL --retry 3 "https://github.com/wasm-bindgen/wasm-pack/releases/download/v${WASM_PACK_VERSION}/wasm-pack-v${WASM_PACK_VERSION}-$(uname -m)-unknown-linux-musl.tar.gz" \
    | tar -xz --strip-components=1 -C /usr/local/cargo/bin --wildcards '*/wasm-pack'
WORKDIR /src/client
COPY client/scripts/setup-vosk.sh scripts/
RUN if [ "$VOSK_MODELS" = true ]; then bash scripts/setup-vosk.sh; fi
COPY client/package.json client/pnpm-lock.yaml client/pnpm-workspace.yaml ./
RUN npm install -g "$(node -p 'require("./package.json").packageManager')" \
    && pnpm install --frozen-lockfile
COPY query /src/query
COPY assets/adif /src/assets/adif
COPY client ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/query/target \
    pnpm run build

FROM rust AS server
ARG GIT_COMMIT=
WORKDIR /src/server
COPY query /src/query
COPY assets/adif /src/assets/adif
COPY server/Cargo.toml server/Cargo.lock server/build.rs ./
COPY server/migrations migrations
COPY server/src src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/server/target \
    cargo build --release --locked \
    && cp target/release/jl1his-logger /usr/local/bin/ \
    && mkdir -p /out/data

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=server /usr/local/bin/jl1his-logger /usr/local/bin/
COPY --from=client /src/client/build /usr/share/jl1his-logger/web
COPY assets/config.example.toml /usr/share/jl1his-logger/
COPY --from=server --chown=nonroot:nonroot /out/data /data
WORKDIR /data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["jl1his-logger"]
CMD ["serve", "--listen", "0.0.0.0:8080", "--web-dir", "/usr/share/jl1his-logger/web"]
