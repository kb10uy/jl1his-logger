# Repository Guidelines

## Overview

jl1his-logger is the QSO log of record for JL1HIS: one Rust server, one SQLite
database, a same-origin SvelteKit PWA and a Tauri desktop entry client. The
design is recorded in
`docs/development/decisions.md`; it is the authority on IDs, synchronization, storage,
permissions, ADIF handling and the Wavelog API v2 compatibility layer, and
this file only summarizes it. `README.md` is the product overview.

The repository contains:

- `server/`: Rust (edition 2024) + axum + sqlx (SQLite). The native API, the
  Wavelog API v2 compatibility layer under `/wl2`, the SSE stream, ADIF import
  and export, passkey sign-in (webauthn-rs), and the admin CLI (`setup`,
  reference data, backup). Serves `client/build` on one origin.
- `client/`: SvelteKit + Svelte 5 (runes) + TypeScript, built with
  `adapter-static` as a client-only SPA/PWA. Log list and search, QSO editing,
  live entry, voice entry, Station Location and
  Instrument editing, ADIF import and export, bulk QSL changes.
- `desktop/`: the desktop entry client, a Tauri v2 app. A SvelteKit app built
  with `adapter-static` like `client/`, and its Rust side in
  `desktop/src-tauri/`, a package of its own.
- `query/`: the query syntax of the log list as a Rust crate (parsing and
  matching a row), used by the server and compiled to WASM for the client.
- `assets/`: `config.example.toml`, and under `adif/` the ADIF specification's
  CSV exports that the server and client generate their ADIF tables from (see
  its README).
- `docs/`: `guide/` for running and talking to the application:
  `operations.md` (building, configuration, the CLI, backups, the container),
  `api.md` (the native API), `query.md` (the search query syntax), `wl2.md`
  (the `/wl2` layer) and `protocol.md` (what the desktop client takes over
  UDP); `development/` for working on it:
  `decisions.md` (the design record) and `schema.md` (the database).
- `fixtures/`: expected files shared by the `server/` and `client/` tests,
  such as ADIF exports.

The server is a single package and `query/` the one shared crate, a path
dependency (see "Structure" in `docs/development/decisions.md`). `desktop/src-tauri/` is
an application, not a shared crate. Do not create a workspace or another
shared crate.

## Architecture

- **The QSO row is the record.** Core fields (call, band, mode, time, station,
  QSL state and so on) are columns; everything else is a JSON string map. A
  field lives in one place, never both. Unknown standard fields and `APP_*`
  are kept. Values stay the ADIF strings they arrived as; normalized forms
  (callsign, time in seconds, frequency in Hz) go to derived columns only.
- **IDs are client-generated UUID v7.** `POST` is idempotent: the same ID
  returns the existing row. Heuristic duplicate detection (same call, band,
  mode, nearby time) warns and never refuses; the check is shared by the
  candidate endpoint and the `warnings` of `POST`.
- **`seq` and `updated_seq` are per Logbook and never reused.** Differences
  are `updated_seq > N` over current rows including tombstones (upsert or
  delete). There is no append-only change log and no history; `revision`
  detects conflicting updates.
- **Station values are baked into the QSO** as `STATION_CALLSIGN` and `MY_*`
  at creation. Editing a Station never rewrites past QSOs; a bulk update that
  states its count does. `station_location_id` is kept separately for
  search. Stations are archived rather than deleted.
- **Instruments are optional equipment** (`MY_RIG`, `MY_ANTENNA`, `TX_PWR` and
  other defaults) of a Logbook, independent of Stations, which never hold
  those three. Their values are baked in before the Station's, so they win;
  `instrument_id` is kept like `station_id`. Without one, nothing fills them.
  `/wl2` QSOs have none.
- **A Logbook may name an area code field** (`area_field`: an ADIF field
  and a label, such as `CNTY` labelled "JCC / JCG"), set by owners. Live
  entry, voice entry and the QSO editor show one field for it and nothing
  without one. JCC / JCG is a setting, not a built-in.
- **FREQ may be missing**; the server never fills it with a representative
  frequency. In the client's editors, choosing a band sets the frequency field
  to the band's representative frequency, and typing a frequency moves the band.
  Several parks are one row with several `MY_POTA_REF` references; the POTA
  export projects one record per park. QSOs are never duplicated.
- **QSL state is the ADIF fields** (`QSL_SENT`, `QSL_RCVD`, `*_VIA`,
  `QSLSDATE`, `QSLRDATE`, `EQSL_QSL_*`, `LOTW_QSL_*`), with no parallel
  booleans. `qsl_required` is `QSL_SENT in {R, Q}`; received is
  `QSL_RCVD in {Y, V}`.
- **ADIF lengths are codepoints by default**, with bytes or UTF-16 units as
  export options. Imports detect the unit for each value and read a file that
  is not UTF-8 as the fallback encoding (Shift_JIS by default); exports can be
  written in Shift_JIS, Windows-1252 or ASCII, with lengths in its bytes.
  Reading and writing go through `adif-manip` (github.com/kb10uy/adif-manip);
  `server/src/domain/adif.rs` maps a QSO to and from its fields.
- **Exports are batches**: record the UUIDs and `revision`s, and apply the
  external result to those rows only.
- **The server searches; the client keeps no copy of the Logbook.** The log
  list sends one text query syntax (`call:JA1*`, `band:40m,20m`, `-mode:FT8`,
  `date:2026-10-01..`, `no:freq`, `adif.TX_PWR:10`, bare words) and pages
  newest first on `(qso_time, id)`, or by `sort` oldest first or by `call_base`. The native API and export filters take the
  same syntax; the CLI does not yet. An invalid term is `400`, never dropped.
  The terms are listed in `docs/guide/query.md`. The syntax
  is one implementation, the `query/` crate: the server turns its terms into
  SQL, and the client runs it as WASM for the chips and to re-test SSE rows.
- **SSE, not WebSocket.** Event `id` is `updated_seq`; reconnection uses
  `Last-Event-ID`, and a client that keeps a copy passes its stored cursor as
  `?after=`. The stream carries the whole Logbook unfiltered; clients
  re-evaluate their view. The web client updates loaded rows in place and only
  counts new rows outside its query. A restored backup changes the cursor
  generation and forces a full resync.
- **Roles.** User: `admin` / `user`, signed in with passkeys only, identified
  by a personal callsign. Membership: `owner` / `operator` per Logbook.
  `admin` manages users, invitations, Logbooks, owners, tokens, reference data,
  setup and backups, and still needs a Membership to read QSOs; a rescue
  operation is explicit and records its reason and actor. `owner` manages its
  Logbook's Stations and Memberships but creates no users. `operator` edits
  and deletes anyone's QSOs and manages Instruments; rewriting the past QSOs
  of a Station or an Instrument is for owners. API tokens belong to a User and are scoped to
  Logbooks; the plaintext is shown once.
- **The `/wl2` layer matches a subset of Wavelog API v2**:
  `station_profile_id`, `since_id` / `lastfetchedid` / `has_more`,
  `format=adif`, `400 validation_error` with `details.duplicate`,
  `lookup?detail=basic`, and the `contest` and `subdivisions` catalog topics.
  `/wl2/api/v2/lookup` is an alias. Compatibility tokens are fixed to one
  Logbook, optionally prefixed `wl2_`. Compatibility `since_id`
  is the insertion `seq`. A Station may carry the Wavelog integer ID `/wl2`
  clients address it by. No Host-based routing.
- **Reference data** (a CTY zip archive holding `cty.dat` and `cty.csv`) is
  installed by an admin with a CLI subcommand or by an upload on the server
  settings screen; the web UI shows versions.
  Updating it never rewrites DXCC or zones already recorded on QSOs.
- **ADIF tables are built in** (bands, modes, QSL values, QSO field names, DXCC
  entities, subdivisions): generated at build time from the ADIF
  3.1.8 CSV exports in `assets/adif/`, by `server/build.rs` and by the client's
  `pnpm run adif`. The web UI gets the subdivisions of one DXCC entity from the
  native API for `STATE` and `MY_STATE` choices; they are not in the bundle.
- **County and area codes are strings** (leading zeros matter, as in JCC /
  JCG numbers).
- **One SQLite database**, WAL and `busy_timeout` set, short write
  transactions. Schema changes are migrations under `server/migrations/`;
  never edit an applied one.
- **Entry keeps only its own QSOs offline.** Entry writes to an IndexedDB
  queue before any network call and sends at once; sync is idempotent on the
  UUID, so retries are always safe. A QSO that could not be sent stays in the
  queue, shown on the screen, until the user sends it; nothing retries in the
  background. Voice entry and live entry share the queue.
- **Every screen has a path**, under `/logbooks/{logbook_id}/...` for a
  Logbook's screens, so it enters the browser history. The log keeps its
  query in `?q=` and the open QSO in `?qso=`, replacing the history entry
  rather than adding one.
- **Secrets stay on the server.** No CSRF tokens: the session is a `__Host-`
  `SameSite=Lax` cookie and state-changing requests must come from
  `public_origin`.

## Code Style

- Use Rust edition 2024.
- Follow standard Rust naming and formatting conventions; `server/rustfmt.toml`
  sets `max_width = 120`.
- Use LF line endings and UTF-8 for all text files, and follow
  `.editorconfig`. There is no `.gitattributes` to correct this after the
  fact, so every edit has to write LF itself. On Windows a tool that opens a
  file in the platform's text mode converts LF to CRLF without reporting it:
  Python's `pathlib.write_text` and `open(path, "w")`, PowerShell's `>`
  redirection, and `Set-Content` all do. Write bytes instead, or edit in place
  with a tool that preserves what is already there.
- Check `git diff --stat` before committing. A small edit that reports the
  whole file as changed has had its line endings rewritten.
- Combine imports from the same crate into a single `use` statement within each
  module scope, except when different `cfg` attributes require separate imports.
- Avoid comments by default. Add comments only when explicitly requested by the
  user.
- Prefer the smallest correct implementation and avoid speculative abstractions.
- Prefer `modname.rs` and `modname/` style over `mod.rs` style.
- Use `rstest` for parameterized tests, fixtures, and test cases where they
  improve coverage or reduce repetition.
- In `server/`, pure logic (bands, callsigns, QSO records, duplicate
  detection, ADIF) lives under `src/domain/` and must not touch the
  database, network or file system; `config`, `db` and `web` depend on it,
  never the other way round.
- In `client/`, pure logic (ADIF, QSO records, the voice DSL, frequency
  resolution) must not touch the DOM, audio, storage or network, and has unit
  tests next to it (`*.test.ts`). Prettier (single quotes, width 100,
  indentation from `.editorconfig`) is the formatter.
- In `client/`, import `src/lib` through `#lib/...` with the file extension
  (`#lib/api/api.ts`, `#lib/state/session.svelte.ts`); SvelteKit 3 has no
  `$lib`. UI parts are Svelte components under `src/lib/components/ui/`
  styled with Tailwind v4 tokens from `src/theme.css`; bits-ui is used only
  for menus and dialogs. `desktop/` imports the same `client/src/theme.css`,
  so its screens share the tokens, fonts and base styles.
- `desktop/` reuses the web client's pure logic and API types (the QSO record,
  bands, modes, contest exchanges, the queue) as `#client/...`, which maps onto
  `client/src/lib` through a `link:` dependency on `client/`. Install
  `client/` first; the desktop's `dev`, `build`, `check` and `test` run the
  client's `adif` and `messages` before their own steps. Text that code
  produces comes from the client's messages; the desktop's own components use
  `desktop/messages/`.
- `client/src/lib/query/wasm/` is generated by `pnpm run wasm` (wasm-pack) and
  not committed; `dev`, `build`, `check` and `test` run it first, so they
  need Rust with the `wasm32-unknown-unknown` target and `wasm-pack`.
- `client/src/lib/adif/` is generated by `pnpm run adif` from `assets/adif/`
  and not committed; `dev`, `build`, `check` and `test` run it first. The
  server generates the same tables in `build.rs`. Hand-curated lists (band
  groups, mode choices, QSL labels) stay hand-written and are tested against
  the generated tables. Never edit the CSVs by hand.
- Keep the ADIF renderers and API payload types of `server/` and `client/` in
  sync; their tests share expected strings, kept as files under `fixtures/`.
- Code, comments and commit messages are in English. UI text is localized
  (see "Localization" in `docs/development/decisions.md`): write it in
  `client/messages/en/<area>.json` with its Japanese in
  `client/messages/ja/<area>.json`, and use it as `m.<key>()` from
  `#lib/paraglide/messages.js`, generated by `pnpm run messages` (also run by
  `check` and `test`; `dev` and `build` compile it through the Vite plugin) and
  not committed. Pure logic may call messages, but never at module top level.
  `desktop/` does the same with its own `desktop/messages/`.

## Documentation

- Write documentation under `docs/` in English or Japanese: `docs/guide/`
  for operators and the programs that talk to the application, `docs/development/`
  for the design and the code.
- When a change departs from a decision in `docs/development/decisions.md`, or settles
  something listed under "Remaining Work" there, update that file in the same
  change.
- Mention Wavelog only where it concerns the `/wl2` compatibility layer.
- When a change alters behavior, APIs, architecture or anything else
  documented, update the relevant documentation in the same change.

## Commits, Issues and PRs

- Use imperative mood. Do not use conventional commit messages.
- Name branches in lowercase, hyphenated, under `feature/`, `fix/`,
  `refactor/`, `docs/` or `test/`.

## Build and Test

CI (`.github/workflows/ci.yml`) runs the same. Before completing a code
change, run at minimum:

```text
cd query
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --lib --features wasm --target wasm32-unknown-unknown -- -D warnings
cargo test

cd server
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build

cd client
pnpm install --frozen-lockfile
pnpm run lint
pnpm run check
pnpm test

cd desktop
pnpm install --frozen-lockfile
pnpm run lint
pnpm run check
pnpm test
pnpm run build

cd desktop/src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The Tauri crate embeds `desktop/build`, so build the desktop frontend before
running Clippy on it. The desktop frontend imports `client/src/lib`, so install
`client/` before checking `desktop/`.

`cargo deny check licenses` and `cargo deny check advisories` run in `server/`
against `server/deny.toml`.
