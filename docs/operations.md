# Operations

How to build, configure and run jl1his-logger, and what the administration CLI
does. The reasons behind these choices are in [decisions.md](decisions.md),
mostly under "Migrations and Deployment", "Backup and Restore" and "Reference
Data".

## Building and Starting

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

The client build compiles `query/` to WASM first, so it needs Rust with the
`wasm32-unknown-unknown` target and
[wasm-pack](https://github.com/drager/wasm-pack). Without the speech models
the rest of the app works and voice entry says the model is missing.

Passkeys need HTTPS (or `http://localhost`), so put a TLS-terminating reverse
proxy in front; it is also where HSTS and any request rate limits belong, since
the server applies neither. `GET /api/health` answers `{"status": "ok"}` while
the database can be read, for the proxy or the container runtime to probe.

## Configuration

Everything is set in one TOML file, `config.toml` in the working directory or
the file given with `-c`/`--config`; nothing is read from the environment but
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
| `adif.length` | Length counting for exported ADI, `codepoints` (default), `bytes` or `utf16`; an import that is not told a unit tries it first |
| `adif.fallback_encoding` | How an import reads a file that is not UTF-8: `shift_jis` (default), `windows-1252`, `ascii` or `utf-8` |

## Migrations

Before applying pending migrations to an existing database, the server writes a
`VACUUM INTO` snapshot to the `snapshots` directory next to the database, named
after the first pending migration. Migrations only go forward; rolling back
means restoring that snapshot with the matching binary.

```sh
jl1his-logger migrate         # apply pending migrations explicitly
```

## The CLI

The first admin is created by `setup`, which prints a one-time registration
URL:

```sh
jl1his-logger setup --callsign JL1HIS
```

The other commands act as the user named with `--as`, and go through the same
permission checks as the web app. `--logbook`, `--station` and `--instrument`
take an ID or a name; `--logbook` and `--station` may be left out when there is
only one. `jl1his-logger help <command>` lists the options of each.

| Command | Does |
| --- | --- |
| `user` | Adds, lists, invites, disables and enables users and changes their instance role (admin) |
| `logbook` | Creates, lists and deletes Logbooks, and makes a user an owner as a recorded rescue |
| `member` | Lists the members of a Logbook; sets and removes them (owner) |
| `station` | Manages the Stations of a Logbook and rewrites their past QSOs (owner); marks your favorites |
| `instrument` | Manages the Instruments of a Logbook and marks your favorites; rewriting their past QSOs is for owners |
| `token` | Issues, lists and revokes your API tokens |
| `import` | Imports new QSOs or merges QSL confirmations from an ADIF file |
| `export` | Writes ADIF as an export batch, lists batches and applies their results |
| `settings` | Reads and writes instance settings (admin) |
| `reference` | Lists and installs reference data |
| `backup`, `restore` | Copies the database, or replaces it with a copy |

```sh
jl1his-logger --as JL1HIS logbook add JL1HIS --owner JL1HIS
jl1his-logger --as JL1HIS member set JA1OPR operator --logbook JL1HIS
jl1his-logger --as JL1HIS member list --logbook JL1HIS
jl1his-logger --as JL1HIS station add Home --callsign JL1HIS --field MY_GRIDSQUARE=PM95
jl1his-logger --as JL1HIS instrument add "IC-705 QRP" --rig IC-705 --antenna EFHW --power 5
jl1his-logger --as JL1HIS instrument favorite "IC-705 QRP"                   # listed first for JL1HIS
jl1his-logger --as JL1HIS import log.adi --source new --station Home          # preview
jl1his-logger --as JL1HIS import log.adi --source new --station Home --apply
jl1his-logger --as JL1HIS import portable.adi --source new --station Home --instrument "IC-705 QRP" --apply
jl1his-logger --as JL1HIS import lotwreport.adi --source lotw --apply --rejects rejects.adi
jl1his-logger --as JL1HIS import hamlog.adi --source new --station Home --encoding shift_jis
jl1his-logger --as JL1HIS export write pota --park JP-0001 --from 2026-10-22 --until 2026-10-22 -o pota.adi
jl1his-logger --as JL1HIS export write qsl-card --qsl-sent R,Q -o cards.adi
jl1his-logger --as JL1HIS export write lotw --encoding ascii --unencodable replace -o lotw.adi
jl1his-logger --as JL1HIS export apply <batch>                                # mark them sent
jl1his-logger --as JL1HIS token issue logger --logbook JL1HIS
jl1his-logger --as JL1HIS settings list                                       # instance settings (admin)
```

### Import

An import previews unless given `--apply`. New QSOs get IDs derived from their
records, so rerunning an interrupted import adds only what is missing. A QSL
merge (`lotw`, `eqsl`, `card`) only marks matched QSOs as received and reports
unmatched and ambiguous records instead of guessing. See "ADIF Import" in
decisions.md for the matching rules.

### Wavelog API v2 Clients

A client written for Wavelog API v2 reaches the `/wl2` layer
([wl2.md](wl2.md)) with a `wl2` token fixed to one Logbook, and addresses
Stations by the Wavelog IDs they are given:

```sh
jl1his-logger --as JL1HIS station update Home --wavelog-id 1
jl1his-logger --as JL1HIS token issue field-logger --kind wl2 --wl2-prefix --logbook JL1HIS
```

### Reference Data

Reference data (`cty`, `subdivisions`, `parks`) is installed from local files
by an admin and goes where `[reference]` in the configuration points. The
running server picks up a new version without a restart.

`cty` is a CTY zip archive from [country-files.com](https://www.country-files.com/),
either Big CTY (`bigcty-YYYYMMDD.zip`) or the contest files (`cty-NNNN.zip`),
installed as downloaded: the server takes `cty.dat` and `cty.csv` from it. An
admin can also upload it on the server settings screen of the web app.

```sh
jl1his-logger --as JL1HIS reference install cty bigcty-20261023.zip   # version from the =VER entry of its cty.dat
jl1his-logger --as JL1HIS reference install parks parks.toml --version 2026-10
jl1his-logger --as JL1HIS reference list
```

#### Upgrading from a plain cty.dat

Earlier versions installed `cty` as a plain `cty.dat` and kept the DXCC codes
in a separate `dxcc` kind (`assets/dxcc.toml`). Migration 9 forgets both, so
after the upgrade lookups answer without the entity and new QSOs get no `DXCC`,
`CQZ`, `ITUZ` or `CONT` until `cty` is installed again from an archive:

1. Remove the `dxcc` line from `[reference]` in `config.toml`; the setting is no
   longer accepted and the server refuses to start with it. Optionally point
   `cty` at `reference/cty.zip`, the new default.
2. Start the new version (or run `migrate`).
3. Install a CTY archive with `reference install cty <archive>.zip` or from the
   server settings screen.
4. Delete the old `cty.dat` and `dxcc.toml` from the reference directory if
   `cty` no longer points at them.

### Backup and Restore

`backup` copies the database while the server runs; `restore` needs the server
stopped, keeps the replaced database in `snapshots`, signs everyone out and
makes every client sync again:

```sh
jl1his-logger backup --keep 14                  # into backups/ next to the database
jl1his-logger restore backups/jl1his-logger-20261022T000000Z.db --yes
```

Scheduling backups and copying them off the host are left to the host (cron, a
systemd timer).

## Container

The `Dockerfile` builds one image holding the server and the built client,
speech models included (`--build-arg VOSK_MODELS=false` leaves them out;
`--build-arg GIT_COMMIT=$(git rev-parse HEAD)` records the commit the server
reports, since `.git` is not part of the build context). It serves on port 8080
and works in the volume at `/data`, which holds `config.toml` next to the
database, reference data, snapshots and backups. The listen address and the web
app come from the image's `serve` arguments, so the configuration needs only
`server.public_origin`. The image is based on distroless and runs as its
`nonroot` user (UID 65532) without a shell, so `docker exec` runs the binary
itself, files go in and out with `docker cp`, and a bind-mounted `/data` must be
writable by UID 65532. `assets/config.example.toml` is in
`/usr/share/jl1his-logger`.

The Image workflow pushes it to `ghcr.io/kb10uy/jl1his-logger`: a `v*` tag
publishes its version and `latest`, a manual run on `master` publishes `edge`,
and every build is also tagged `sha-<commit>`.

```sh
docker build -t jl1his-logger .
docker create --name jl1his-logger -p 127.0.0.1:8080:8080 -v jl1his-logger:/data jl1his-logger
printf '[server]\npublic_origin = "https://log.example.com"\n' > config.toml
docker cp config.toml jl1his-logger:/data/config.toml
docker start jl1his-logger
docker exec jl1his-logger jl1his-logger setup --callsign JL1HIS
docker cp bigcty-20261023.zip jl1his-logger:/data/bigcty-20261023.zip
docker exec jl1his-logger jl1his-logger --as JL1HIS reference install cty /data/bigcty-20261023.zip
```
