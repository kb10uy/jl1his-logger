# JL1HIS Logger

The QSO log of record for JL1HIS: one server, one SQLite database and a same-origin
web app, built to replace [Wavelog](https://www.wavelog.org/) as the single source
of truth that eQSL, contest submissions, QSL cards and POTA are fed from.

Wavelog does far more than this station needs, and its web interface is not what
the operator wants to type into. This project keeps the four API resources the
station's own clients actually use (`qso`, `station`, `lookup`, `catalog`), speaks
ADIF in and out, and drops the rest: no dashboards, maps, DX cluster, QRZ lookups,
CAT control, contest modules, awards or satellites.

The design is recorded in [docs/decisions.md](docs/decisions.md). The server so far
opens its database, applies migrations, serves the built client, signs users in
with passkeys and has the administration CLI; this README describes what the
repository is going to hold.

## What it does

- Stores QSOs per Logbook (a personal or club station) with their Station Location,
  keeping every ADIF field, including unknown and `APP_*` ones, as the string it
  arrived as.
- Writes, reads, filters and exports QSOs through its own API; streams changes over
  SSE so every open client follows the log.
- Imports ADIF, both for the first migration from Wavelog and to merge QSL fields
  (paper, eQSL and LoTW confirmations, each kept separately) into existing QSOs.
- Looks up a callsign's DXCC entity from `cty.dat` and the station's own previous
  QSOs with it (name, grid, QTH).
- Serves a SvelteKit PWA on the same origin: log list and search, single-QSO
  editing, live QSO entry, push-to-talk voice entry (absorbing
  [speech-to-qso](https://github.com/kb10uy/speech-to-qso)), Station Location
  editing, ADIF import and export, and bulk QSL changes.
- Offers a Wavelog API v2 compatible layer under `/wl2` so the existing clients
  keep working until they move to the native API.

Users sign in with passkeys only. Instance roles are `admin` and `user`; Logbook
roles are `owner` and `operator`. API tokens belong to a user and are scoped to
Logbooks.

## Clients

| Client | Today | Plan |
| --- | --- | --- |
| [hislog](https://github.com/kb10uy/hislog) | Wavelog API: `POST qso`, `GET qso?format=adif&since_id=`, `GET station`, `GET lookup`, `GET catalog` | `/wl2` until it has a persistent UUID queue, then the native API |
| speech-to-qso | Wavelog API: `POST qso`, `GET station`, `GET qso` | Absorbed into this web app |
| [grayline](https://github.com/kb10uy/grayline) | Wavelog API: `GET lookup` with a `wl2_` token | `/wl2/api/v2/lookup` for the foreseeable future |
| [wavelog-tools](https://github.com/kb10uy/wavelog-tools) | Wavelog API: `GET qso?format=adif`, `GET catalog` (subdivisions) | `/wl2` until the native CLI covers export and subdivisions |
| [qso-tools](https://github.com/kb10uy/qso-tools) | ADIF only | Unchanged; fed by this server's ADIF export with `--lenient codepoints` |

## Layout

- `server/` — Rust (axum, SQLite). The API, the Wavelog compatibility layer, the
  SSE stream, ADIF import and export, and the admin CLI (setup, reference data,
  backup). Serves `client/build` on the same origin.
- `client/` — SvelteKit + TypeScript PWA, built with `adapter-static` as a
  client-only app with offline entry.
- `query/` — the log's query syntax as a Rust crate, used by the server and
  compiled to WASM for the client.
- `assets/` — example files, such as `config.example.toml`.
- `docs/` — development documentation. `decisions.md` is the design record,
  `schema.md` the database and `api.md` the native API.
- `fixtures/` — expected files shared by the server and client tests.

## Running

```sh
cd client
pnpm install --frozen-lockfile
pnpm run models               # the Vosk speech models for voice entry, about 90 MB
                              # (bash; on Windows: pwsh scripts/setup-vosk.ps1)
pnpm run build                # → client/build

cd ../server
mkdir -p data
cp ../assets/config.example.toml data/config.toml   # edit public_origin and web_dir
cargo run --release -- -c data/config.toml serve
```

Everything is set in one TOML file, `config.toml` in the working directory or the
file given with `-c`/`--config`; nothing is read from the environment but
`RUST_LOG`. Paths in it are relative to its own directory, so a data directory
holding `config.toml` keeps the database, reference data, snapshots and backups
next to it. `assets/config.example.toml` lists every setting with its default.

| Setting | Description |
| --- | --- |
| `server.listen` | Listen address (default `127.0.0.1:8080`); `serve --listen` overrides it |
| `server.public_origin` | URL the app is reached at, with a DNS hostname (IP addresses are refused; use `http://localhost` locally). Passkeys are bound to its host; do not change it later. Needed by `serve`, `setup` and `user invite` |
| `server.web_dir` | The built app (`client/build`); `serve --web-dir` overrides it. Without it only the API runs |
| `server.auto_migrate` | Apply pending migrations when `serve` starts (default `true`). With `false`, pending migrations stop the server until `migrate` runs |
| `database.path` | The SQLite database (default `jl1his-logger.db`) |
| `reference.*` | Where reference data is installed (default under `reference/`) |
| `adif.length` | Length counting for exported ADI, `codepoints` (default) or `bytes` |

Before applying pending migrations to an existing database, the server writes a
`VACUUM INTO` snapshot to the `snapshots` directory next to the database, named
after the first pending migration. Migrations only go forward; rolling back means
restoring that snapshot with the matching binary.

```sh
jl1his-logger migrate         # apply pending migrations explicitly
```

Passkeys need HTTPS (or `http://localhost`), so put a TLS-terminating reverse proxy
in front; it is also where HSTS and any request rate limits belong, since the server
applies neither. The first admin is created by the setup command, which prints a
one-time registration URL:

```sh
jl1his-logger setup --callsign JL1HIS
```

The other commands act as the user named with `--as`, and go through the same
permission checks as the web app. `--logbook` and `--station` take an ID or a name,
and may be left out when there is only one.

```sh
jl1his-logger --as JL1HIS logbook add JL1HIS --owner JL1HIS
jl1his-logger --as JL1HIS member set JA1OPR operator --logbook JL1HIS
jl1his-logger --as JL1HIS member list --logbook JL1HIS
jl1his-logger --as JL1HIS station add Home --callsign JL1HIS --field MY_GRIDSQUARE=PM95 --wavelog-id 1
jl1his-logger --as JL1HIS import wavelog.adi --source new --station Home      # preview
jl1his-logger --as JL1HIS import wavelog.adi --source new --station Home --apply
jl1his-logger --as JL1HIS import lotwreport.adi --source lotw --apply --rejects rejects.adi
jl1his-logger --as JL1HIS export write pota --park JP-0001 --from 2026-10-22 --until 2026-10-22 -o pota.adi
jl1his-logger --as JL1HIS export write qsl-card --qsl-sent R,Q -o cards.adi
jl1his-logger --as JL1HIS export apply <batch>                                # mark them sent
jl1his-logger --as JL1HIS token issue grayline --kind wl2 --wl2-prefix --logbook JL1HIS
jl1his-logger --as JL1HIS reference install cty cty.dat                       # version from its =VER entry
jl1his-logger --as JL1HIS reference install dxcc assets/dxcc.toml --version 2026-10
jl1his-logger --as JL1HIS reference list
jl1his-logger --as JL1HIS settings list                                       # instance settings (admin)
```

Reference data (`cty`, `dxcc`, `subdivisions`, `parks`) is installed from local
files by an admin and goes where `[reference]` in the configuration points. The
running server picks up a new version without a restart.

An import previews unless given `--apply`. New QSOs get IDs derived from their
records, so rerunning an interrupted import adds only what is missing. A QSL merge
(`lotw`, `eqsl`, `card`) only marks matched QSOs as received and reports unmatched
and ambiguous records instead of guessing.

`backup` copies the database while the server runs; `restore` needs the server
stopped, keeps the replaced database in `snapshots`, signs everyone out and makes
every client sync again:

```sh
jl1his-logger backup --keep 14                  # into backups/ next to the database
jl1his-logger restore backups/jl1his-logger-20261022T000000Z.db --yes
```

## Development

The client uses [pnpm](https://pnpm.io), at the version pinned in
`client/package.json`. Its build compiles `query/` to WASM first, so it needs
Rust with the `wasm32-unknown-unknown` target and
[wasm-pack](https://github.com/drager/wasm-pack).
Voice entry needs the speech models in `client/static/models`, fetched by
`pnpm run models`; without them the rest of the app works and voice entry says
the model is missing.

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

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE](LICENSE))
- MIT License ([LICENSE.MIT](LICENSE.MIT))

at your option.
