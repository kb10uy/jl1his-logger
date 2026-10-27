# JL1HIS Logger

The QSO log of record for JL1HIS: one server, one SQLite database and a
same-origin web app. It is the single source of truth that eQSL, LoTW, contest
submissions, QSL cards and POTA are fed from, and it speaks ADIF in and out.

It does only what this station needs. Dashboards, maps, DX cluster, QRZ and
HamQTH lookups, CAT control, contest modules, awards and satellites are out of
scope.

## What It Does

- Stores QSOs per Logbook (a personal or club station) with their Station
  Location and, optionally, the Instrument (rig, antenna, TX power) used,
  keeping every ADIF field, including unknown and `APP_*` ones, as the string
  it arrived as.
- Writes, reads, searches and exports QSOs through its own API, with one text
  query syntax (`call:JA1* band:40m,20m -mode:FT8`); streams changes over SSE so
  every open client follows the log.
- Imports ADIF, both to load existing logs and to merge QSL confirmations
  (paper, eQSL and LoTW, each kept separately) into existing QSOs. Exports are
  recorded as batches, so sending QSL cards or uploads is marked on exactly the
  QSOs that were exported.
- Looks up a callsign's DXCC entity from `cty.dat` and the station's own
  previous QSOs with it (name, grid, QTH).
- Serves a SvelteKit PWA on the same origin: log list and search, single-QSO
  editing, live QSO entry, push-to-talk voice entry, Station Location and
  Instrument editing, ADIF import and export, and bulk QSL changes, in English
  or Japanese. Entry keeps working offline.
- Offers a Wavelog API v2 compatible layer under `/wl2`, so logging clients
  written for Wavelog can log here.

Users sign in with passkeys only. Instance roles are `admin` and `user`;
Logbook roles are `owner` and `operator`. API tokens belong to a user and are
scoped to Logbooks.

## Layout

- `server/` — Rust (axum, SQLite). The native API, the `/wl2` compatibility
  layer, the SSE stream, ADIF import and export, and the admin CLI (setup,
  reference data, backup). Serves `client/build` on the same origin.
- `client/` — SvelteKit + TypeScript PWA, built with `adapter-static` as a
  client-only app with offline entry.
- `query/` — the log's query syntax as a Rust crate, used by the server and
  compiled to WASM for the client.
- `assets/` — `config.example.toml`.
- `docs/` — development documentation.
- `fixtures/` — expected files shared by the server and client tests.

## Documentation

- [docs/operations.md](docs/operations.md) — building, configuring and running
  the server, the CLI, backups and the container image.
- [docs/decisions.md](docs/decisions.md) — the design record and the reasons
  behind it.
- [docs/api.md](docs/api.md) — the native API under `/api`.
- [docs/wl2.md](docs/wl2.md) — the Wavelog API v2 compatibility layer under
  `/wl2`.
- [docs/schema.md](docs/schema.md) — the SQLite schema.

## Quick Start

```sh
cd client
pnpm install --frozen-lockfile
pnpm run build                # → client/build

cd ../server
mkdir -p data
cp ../assets/config.example.toml data/config.toml   # edit public_origin and web_dir
cargo run --release -- -c data/config.toml setup --callsign JL1HIS
cargo run --release -- -c data/config.toml serve
```

`setup` prints a one-time link to register the first admin's passkey. Passkeys
need HTTPS or `http://localhost`. See [docs/operations.md](docs/operations.md)
for the rest, including the container image.

## Development

The client uses [pnpm](https://pnpm.io), at the version pinned in
`client/package.json`. Its build compiles `query/` to WASM first, so it needs
Rust with the `wasm32-unknown-unknown` target and
[wasm-pack](https://github.com/drager/wasm-pack). The server and the client
generate their ADIF tables from the CSV files in `assets/adif/` when they
build; see its README to update them to a new ADIF version.
Voice entry needs the speech models in `client/static/models`, fetched by
`pnpm run models` (bash; on Windows `pwsh scripts/setup-vosk.ps1`); without
them the rest of the app works and voice entry says the model is missing.
The favicon and app icons in `client/static` are generated from
`client/static/logo.svg` by `pnpm run icons`, which needs
[ImageMagick](https://imagemagick.org) with librsvg.

```sh
cd client
pnpm install
pnpm run dev                  # Vite; /api is forwarded to the server at API_SERVER
pnpm run lint
pnpm run check
pnpm test

cd ../query
cargo test

cd ../server
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`AGENTS.md` holds the repository guidelines and the full set of checks CI
runs.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE](LICENSE))
- MIT License ([LICENSE.MIT](LICENSE.MIT))

at your option.
