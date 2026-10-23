# jl1his-logger

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
- `assets/` — example files, such as `config.example.toml`.
- `docs/` — development documentation. `decisions.md` is the design record,
  `schema.md` the database and `api.md` the native API.
- `fixtures/` — expected files shared by the server and client tests.

## Running

```sh
cd client
npm ci
npm run build                 # → client/build

cd ../server
cp .env.example .env          # edit; read from the working directory or a parent
cargo run --release -- serve
```

| Environment variable | Description |
| --- | --- |
| `LISTEN` | Listen address (default `127.0.0.1:8080`) |
| `PUBLIC_ORIGIN` | URL the app is reached at, with a DNS hostname (IP addresses are refused; use `http://localhost` locally). Passkeys are bound to its host; do not change it later |
| `WEB_DIR` | The built app (`client/build`). Without it only the API runs |
| `DATABASE_PATH` | The SQLite database (default `data/jl1his-logger.db`) |
| `CONFIG_PATH` | TOML configuration, also `-c`/`--config` (see `assets/config.example.toml`). Defaults apply without it |
| `AUTO_MIGRATE` | Apply pending migrations when `serve` starts (default `true`). With `false`, pending migrations stop the server until `migrate` runs |

Before applying pending migrations to an existing database, the server writes a
`VACUUM INTO` snapshot to the `snapshots` directory next to the database, named
after the first pending migration. Migrations only go forward; rolling back means
restoring that snapshot with the matching binary.

```sh
jl1his-logger migrate         # apply pending migrations explicitly
```

Passkeys need HTTPS (or `http://localhost`), so put a TLS-terminating reverse proxy
in front. The first admin is created by the setup command, which prints a one-time
registration URL:

```sh
jl1his-logger setup --callsign JL1HIS
```

The other commands act as a named user, given with `--as` or `CLI_CALLSIGN`, and go
through the same permission checks as the web app. `--logbook` and `--station` take
an ID or a name, and may be left out when there is only one.

```sh
export CLI_CALLSIGN=JL1HIS
jl1his-logger logbook add JL1HIS --owner JL1HIS
jl1his-logger member set JA1OPR operator --logbook JL1HIS
jl1his-logger member list --logbook JL1HIS
jl1his-logger station add Home --callsign JL1HIS --field MY_GRIDSQUARE=PM95 --wavelog-id 1
jl1his-logger import wavelog.adi --source new --station Home      # preview
jl1his-logger import wavelog.adi --source new --station Home --apply
jl1his-logger import lotwreport.adi --source lotw --apply --rejects rejects.adi
jl1his-logger export write pota --park JP-0001 --from 2026-10-22 --until 2026-10-22 -o pota.adi
jl1his-logger export write qsl-card --qsl-sent R,Q -o cards.adi
jl1his-logger export apply <batch>                                # mark them sent
jl1his-logger token issue grayline --kind wl2 --wl2-prefix --logbook JL1HIS
jl1his-logger reference install cty cty.dat                       # version from its =VER entry
jl1his-logger reference install dxcc dxcc.toml --version 2026-10
jl1his-logger reference list
```

Reference data (`cty`, `dxcc`, `subdivisions`, `parks`) is installed from local
files by an admin and goes where `[reference]` in the configuration points; paths
there are relative to the working directory. The running server picks up a new
version without a restart.

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

```sh
cd client
npm install
npm run dev                   # Vite; /api is forwarded to the server at API_SERVER
npm run lint
npm run check
npm test

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
