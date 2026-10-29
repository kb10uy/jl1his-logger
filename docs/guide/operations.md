# Operations

How to build, configure and run jl1his-logger, and what its small CLI does.
The reasons behind these choices are in [decisions.md](../development/decisions.md),
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
[wasm-pack](https://github.com/drager/wasm-pack). The server and the client
both generate their ADIF tables from `assets/adif/` as they build, so a build
needs the whole repository, not `server/` or `client/` alone. Without the speech models
the rest of the app works and voice entry says the model is missing.

Passkeys need HTTPS (or `http://localhost`), so put a TLS-terminating reverse
proxy in front; it is also where HSTS and any request rate limits belong, since
the server applies neither. Limit `/api/auth/` above all: a flood of sign-in
starts can push a sign-in in progress out of the server's table, and the user
has to start again. `GET /api/health` answers `{"status": "ok"}` while
the database can be read, for the proxy or the container runtime to probe.

## Desktop Client

```sh
cd client
pnpm install --frozen-lockfile
cd ../desktop
pnpm install --frozen-lockfile
pnpm tauri dev                # Vite on port 1420 inside a Tauri window
pnpm tauri build --no-bundle  # → desktop/src-tauri/target/release/jl1his-logger-desktop.exe
pnpm tauri build --bundles deb  # on Linux → desktop/src-tauri/target/release/bundle/deb/
```

The executable is handed out alone, without an installer, and on Linux as a
Debian package. The Desktop workflow builds both for a `v*` tag, the package on
Ubuntu 22.04 so that it runs on that glibc and newer, and attaches them to that
tag's GitHub Release as `jl1his-logger-desktop-windows-x64.exe` and
`jl1his-logger-desktop-linux-x64.deb`, creating the Release with
generated notes when there is none yet; a manual run keeps them as the run's
artifacts only. The names carry no version, so a new one overwrites the old
file and the latest is always at
`https://github.com/kb10uy/jl1his-logger/releases/latest/download/<name>`.
To upgrade, quit the running copy from the tray and replace the file, or on
Linux install the new package over the old one
(`sudo apt install ./jl1his-logger-desktop-linux-x64.deb`); the settings, the
token and unsent QSOs carry over. Its notifications on Windows
are balloons of its tray icon, which Windows shows as toasts under the
executable's own name and icon. On Linux the token needs a Secret Service
(GNOME Keyring, KWallet) and the tray an AppIndicator host.

It signs in with the server's URL and a Native API token issued in the web
client under Settings › API tokens, or with `jl1his-logger token issue`. Its
settings are `config.toml` in the app's configuration directory
(`%APPDATA%\org.kb10uy.jl1his-logger\` on Windows; `config.dev.toml` for a
debug build), or the file given as `--config <path>`
(`pnpm tauri dev -- -- --config dev.toml`). The token is kept in the system's
credential store, not in that file. The rest of it is set on the settings
screen, or by hand:

```toml
language = "ja"              # en or ja; without it, the operating system's
theme = "dark"               # light or dark; without it, the operating system's
close_to_tray = true         # closing the window leaves it running in the tray

[server]
url = "https://log.example.com"

[rig]
enabled = true               # read the rig through rigctld
host = "127.0.0.1"
port = 4532
poll_ms = 1000

[rig.modes]                  # the logged mode for a rig mode, as MODE|SUBMODE
PKTUSB = "MFSK|FT4"
FM = "DIGITALVOICE|C4FM"

[[listeners]]                # UDP for WSJT-X, ADIF and the client's own JSON
enabled = true
host = "127.0.0.1"
port = 2237

[[listeners]]
enabled = false
host = "127.0.0.1"
port = 2333
subscribe = true             # send the form to programs that subscribe from this machine
```

In WSJT-X, set Settings › Reporting › UDP Server to `127.0.0.1` and the
port of a listener. A listener does not ask who is sending: on any address but
the loopback one, other machines on the network can log QSOs and fill the form
too, and the settings screen says so. With `subscribe`, Share the form on the
settings screen, a listener also sends the form back to programs on this
machine that subscribe to it; it is off by default. What a listener takes,
from WSJT-X, as ADIF text or as the client's own JSON, and what it sends back,
is in [protocol.md](protocol.md).

Its frontend imports the web client's
`client/src/lib`, so `client/` is installed first; the desktop's scripts
generate the client's ADIF tables and messages themselves.

It needs the [Tauri prerequisites](https://tauri.app/start/prerequisites/)
of the platform: WebView2 on Windows, WebKitGTK and its development packages
on Linux. The app icons in `desktop/src-tauri/icons` are generated from
`client/static/logo.svg` by `pnpm tauri icon ../client/static/logo.svg -o
src-tauri/icons`, keeping only the files `tauri.conf.json` lists.

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

The CLI does only what the web app cannot: creating the first admin, and the
database work that needs the server's files. Everything else (users, Logbooks,
members, Stations, Instruments, tokens, imports, exports and reference data) is
on the web app and the native API. `jl1his-logger help <command>` lists the
options of each.

| Command | Does |
| --- | --- |
| `migrate` | Applies pending migrations after snapshotting the database |
| `setup` | Creates the first admin, or lets an existing admin register a passkey again, and prints a registration link |
| `backup`, `restore` | Copies the database, or replaces it with a copy |

The first admin is created by `setup`, which prints a one-time registration
URL:

```sh
jl1his-logger setup --callsign JL1HIS
```

Signed in with that passkey, the admin creates the first Logbook on the
Administration screen of the web app (Logbooks). An admin who has lost every
passkey runs `setup` with their callsign again for a new link; the passkeys
they still have stay registered.

### Reference Data

Reference data (`cty`, the one kind) is uploaded by an admin on the Server
section of the web app's Administration screen and goes where `[reference]` in
the configuration points. The running server picks up a new version without a
restart.

`cty` is a CTY zip archive from [country-files.com](https://www.country-files.com/),
either Big CTY (`bigcty-YYYYMMDD.zip`) or the contest files (`cty-NNNN.zip`),
uploaded as downloaded: the server takes `cty.dat` and `cty.csv` from it, and
its version from the `=VER` entry of `cty.dat`.

#### Upgrading from a plain cty.dat

Earlier versions installed `cty` as a plain `cty.dat` and kept the DXCC codes
in a separate `dxcc` kind (`assets/dxcc.toml`). Migration 9 forgets both, so
after the upgrade lookups answer without the entity and new QSOs get no `DXCC`,
`CQZ`, `ITUZ` or `CONT` until `cty` is installed again from an archive:

1. Remove the `dxcc` line from `[reference]` in `config.toml`; the setting is no
   longer accepted and the server refuses to start with it. Optionally point
   `cty` at `reference/cty.zip`, the new default.
2. Start the new version (or run `migrate`).
3. Upload a CTY archive on the Server section of the web app's Administration
   screen.
4. Delete the old `cty.dat` and `dxcc.toml` from the reference directory if
   `cty` no longer points at them.

#### Upgrading to built-in subdivisions

Subdivisions used to be a reference data kind installed from a TOML file. They
are now built into the server from the ADIF specification (see
`assets/adif/README.md`), and migration 10 forgets the `subdivisions` row:

1. Remove the `subdivisions` line from `[reference]` in `config.toml`; the
   setting is no longer accepted and the server refuses to start with it.
2. Start the new version (or run `migrate`).
3. Delete the old `subdivisions.toml` from the reference directory.

#### Upgrading without parks

POTA park names used to be a `parks` reference data kind that nothing in the
web app displayed. It is removed, and migration 11 forgets its row:

1. Remove the `parks` line from `[reference]` in `config.toml`; the setting is
   no longer accepted and the server refuses to start with it.
2. Start the new version (or run `migrate`).
3. Delete the old `parks.toml` from the reference directory.

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
```

Then sign in, create the first Logbook and upload a CTY archive on the
Administration screen.
