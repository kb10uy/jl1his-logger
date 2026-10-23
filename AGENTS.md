# Repository Guidelines

## Overview

jl1his-logger is the QSO log of record for JL1HIS, replacing Wavelog: one Rust
server, one SQLite database and a same-origin SvelteKit PWA. The design is
recorded in `docs/decisions.md`; it is the authority on IDs,
synchronization, storage, permissions, ADIF handling and the Wavelog
compatibility layer, and this file only summarizes it. `README.md` is the
product overview.

The repository contains:

- `server/`: Rust (edition 2024) + axum + sqlx (SQLite). The native API, the
  Wavelog API v2 compatibility layer under `/wl2`, the SSE stream, ADIF import
  and export, passkey sign-in (webauthn-rs), and the admin CLI (`setup`,
  reference data, backup). Serves `client/build` on one origin.
- `client/`: SvelteKit + Svelte 5 (runes) + TypeScript, built with
  `adapter-static` as a client-only SPA/PWA. Log list and search, QSO editing,
  live entry, voice entry (taken over from speech-to-qso), Station Location
  editing, ADIF import and export, bulk QSL changes.
- `assets/`: example files, such as `config.example.toml`.
- `docs/`: development documentation.
- `fixtures/`: expected files shared by the `server/` and `client/` tests,
  such as ADIF exports.

The server is a single package (see "Structure" in `docs/decisions.md`). Do
not create a workspace or a shared crate.

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
- **FREQ may be missing**; never fill it with a representative frequency.
  Several parks are one row with several `MY_POTA_REF` references; the POTA
  export projects one record per park. QSOs are never duplicated.
- **QSL state is the ADIF fields** (`QSL_SENT`, `QSL_RCVD`, `*_VIA`,
  `QSLSDATE`, `QSLRDATE`, `EQSL_QSL_*`, `LOTW_QSL_*`), with no parallel
  booleans. `qsl_required` is `QSL_SENT in {R, Q}`; received is
  `QSL_RCVD in {Y, V}`.
- **ADIF lengths are codepoints by default**, with bytes as an export option.
  Reading goes through `adif-reader` (github.com/kb10uy/adif-reader); the
  writer moves there as a `write` module once the migration rehearsal has
  settled the export output; until then it is `server/src/domain/adif.rs`.
- **Exports are batches**: record the UUIDs and `revision`s, and apply the
  external result to those rows only.
- **The server searches; the client keeps no copy of the Logbook.** The log
  list sends one text query syntax (`call:JA1*`, `band:40m,20m`, `-mode:FT8`,
  `date:2026-10-01..`, `no:freq`, `adif.TX_PWR:10`, bare words) and pages
  newest first on `(qso_time, id)`. The native API, the CLI and export filters
  take the same syntax. An invalid term is `400`, never dropped.
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
  and deletes anyone's QSOs. API tokens belong to a User and are scoped to
  Logbooks; the plaintext is shown once.
- **The `/wl2` layer matches only what the station's clients use**:
  `station_profile_id`, `since_id` / `lastfetchedid` / `has_more`,
  `format=adif`, `400 validation_error` with `details.duplicate`,
  `lookup?detail=basic`, and the `contest` and `subdivisions` catalog topics.
  `/wl2/api/v2/lookup` is an alias for grayline. Compatibility tokens are
  fixed to one Logbook, prefixed `wl2_` for grayline. Compatibility `since_id`
  is the insertion `seq`. Stations keep a mapping to their old Wavelog
  integer IDs. No Host-based routing.
- **Reference data** (`cty.dat`, bands, modes, subdivisions, POTA parks) is
  updated by CLI subcommands; the web UI shows versions and gets the
  subdivisions of one DXCC entity from the native API for `STATE` and
  `MY_STATE` choices. Updating it never rewrites DXCC or zones already
  recorded on QSOs.
- **JCX codes are strings** (leading zeros matter).
- **One SQLite database**, WAL and `busy_timeout` set, short write
  transactions. Schema changes are migrations under `server/migrations/`;
  never edit an applied one.
- **Local first in the client.** Entry writes to IndexedDB before any network
  call; sync is idempotent on the UUID, so retries are always safe. Voice
  entry and live entry share one offline queue. Whether this stays, with the
  queue sent explicitly, or entry needs a connection is an Open Question in
  `docs/decisions.md`.
- **Every screen has a path**, under `/logbooks/{logbook_id}/...` for a
  Logbook's screens, so it enters the browser history. The log keeps its
  query in `?q=` and the open QSO in `?qso=`, replacing the history entry
  rather than adding one.
- **Secrets stay on the server.** No CSRF tokens: the session is a `__Host-`
  `SameSite=Lax` cookie and state-changing requests must come from
  `PUBLIC_ORIGIN`.

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
- Keep the ADIF renderers, band and mode tables and API payload types of
  `server/` and `client/` in sync; their tests share expected strings, kept as
  files under `fixtures/`.
- Code, comments, UI text and commit messages are in English.

## Documentation

- Write documentation under `docs/` in English or Japanese.
- When a change settles something listed under "Open Questions" in `docs/decisions.md`,
  or departs from a decision there, update that file in the same change.
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
cd server
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build

cd client
npm ci
npm run lint
npm run check
npm test
```

`cargo deny check licenses` and `cargo deny check advisories` run in `server/`
against `server/deny.toml`.
