# Design Decisions

The outcome of the design review as of 2026-10-22: the direction for jl1his-logger, a QSO log of record of our own that replaces Wavelog.

## Background

- Today Wavelog is the single source of truth for contacts. QSOs are recorded through its API and web UI, then exported as ADIF and fed to eQSL, contest summary sheets, QSL card generation and POTA.
- Wavelog does far more than this use needs, and its web UI is unsatisfying.
- The related projects and what they depend on in Wavelog:
  - wavelog-tools: `GET qso?format=adif` (with filters), `GET catalog` (subdivisions)
  - hislog: `POST qso`, `GET qso?format=adif&since_id=&page=`, `GET station`, `GET lookup?detail=basic`, `GET catalog?topic=contest`. Keeps an unsent queue and treats a duplicate response (`400 validation_error` with `details.duplicate`) as sent. Sends no UUID
  - speech-to-qso: `POST qso`, `GET station`, `GET qso` (ADIF, previous contacts, `callsign_match=plain`). Drops the client ID when forwarding
  - grayline: `GET lookup`. Chooses v2 by the `wl2_` prefix of the token. Reads ADIF lengths in bytes
  - qso-tools: takes ADIF as its only input. Requires FREQ. Reads parks from `MY_POTA_REF`. Lengths are bytes by default, with `--lenient codepoints` available
- Only four Wavelog API resources are in use: `qso`, `station`, `lookup` and `catalog`.

## Structure

- A new, separate repository (this one) holding one server, one SQLite database and a same-origin SPA / PWA.
- The server is Rust + axum; the client is SvelteKit. The authentication (passkeys), delivery and offline entry of speech-to-qso serve as the reference.
- One SQLite database. Set WAL and `busy_timeout`, and keep write transactions short.

## Required Features

- QSO store, Station Location, Logbook, User, Membership, API tokens.
- Write API (create, update, delete), read API (filters and ADIF output), differential fetch, SSE.
- ADIF import (the first migration, and merging QSL fields into existing QSOs).
- Confirmation state (paper QSL, eQSL and LoTW held separately).
- Reference data: cty.dat, frequencies and bands, modes and submodes, subdivisions, POTA parks.
- lookup: entity information from cty.dat and previous contacts in the own log (name / grid / QTH).
- Web UI: log list and search, single-QSO editing, Live QSO entry, voice entry (absorbing speech-to-qso), Station Location editing, ADIF import and export, bulk QSL state changes.

## Dropped

Dashboards, maps, DX cluster, QRZ / HamQTH integration, CAT (hislog has rigctld), contest modules (qso-tools has loggen), awards (when needed, tallied from ADIF with qso-tools), satellites, themes, organization hierarchies and fine-grained permissions.

## Decisions

### IDs and Duplicate Detection

- The QSO ID is a client-generated UUID v7. `POST` is idempotent: resending the same ID returns the existing row.
- Heuristic duplicate detection (same call, same band, same mode, nearby time) never refuses a record and only warns. It is returned both by a dedicated endpoint (checking a candidate during entry) and in the `warnings` of the `POST` response, with the detection shared between them.
- Each Logbook has server-assigned integers `seq` (insertion order) and `updated_seq` (update order). They are never reused, even after deletion.

### Change History

- There is no append-only change log.
- A row carries `updated_seq`, `revision`, `updated_by`, `updated_at` and a deletion tombstone. Tombstones are never removed.
- The cursor generation is held once per Logbook, not on each row; every differential response and SSE event reports it next to `updated_seq`.
- A differential fetch returns the current rows with `updated_seq > N`, deleted ones included (upsert / delete). Intermediate changes and the state at a past moment are not provided.
- Conflicting updates are detected by `revision`.
- Should history become necessary, it can be added later with SQLite triggers that stack the JSON of the row before each update. The shape of the API does not change.

### Own Station Information

- When a QSO is created, the Station's defaults are baked into the row as `STATION_CALLSIGN` / `MY_*`. Editing the Station does not change past ADIF.
- `station_location_id` is held separately for classification and search.
- Past rows are corrected with an explicit bulk update that states the number of affected rows.
- Stations are archived rather than deleted.

### Storage Shape

- Core fields (call, band, mode, date and time, station, QSL state and so on) are columns; the rest is a JSON string map.
- A field that has a column is not duplicated in the JSON. Unknown standard fields and `APP_*` are not dropped.
- Values are kept as their ADIF strings; normalized forms for search (the normalized callsign, time in seconds, frequency in Hz) go to derived columns.
- Composite foreign keys enforce that a QSO belongs to the same Logbook as its Station, and that an export batch contains only QSOs from its own Logbook.
- The tables are described in [schema.md](schema.md).

### FREQ and Parks

- A QSO without FREQ can be saved. qso-tools is to be changed to make FREQ optional. No representative frequency is filled in.
- Several parks are one row in the store, with several references in `MY_POTA_REF`; the POTA submission export projects one record per park with `MY_SIG_INFO` expanded. QSOs are not duplicated.

### Users and Permissions

- Instance roles (held by a User): `admin` / `user`.
- Logbook roles (held by a Membership): `owner` / `operator`.
- A User is a person. Signs in with a passkey. Has a personal callsign.
- A Logbook is the unit of a personal or club station and owns its Station Locations.
- A QSO's `OPERATOR` defaults to the callsign of the person who entered it, and `created_by` / `updated_by` record the User.
- `Membership.owner` is an application administration right, separate from ADIF's `OWNER_CALLSIGN`.
- An admin's responsibilities lie outside Logbooks: creating and disabling users, invitation links, passkey re-registration, creating and deleting Logbooks and reassigning their owner, revoking other people's tokens, reference data updates, initial setup, backups, and instance settings.
- An owner manages the Stations and Memberships of its own Logbook but cannot create Users.
- Even an admin needs a Membership to read QSOs. A rescue operation is explicit and records its reason and actor.
- An operator can edit and delete QSOs entered by others.
- API tokens belong to a User and are limited to Logbooks by scopes. The plaintext is shown only when issued.
- The first admin is created by a CLI setup command, which issues a one-time registration URL.

### Wavelog API v2 Compatibility Layer

- Mounted under the path prefix `/wl2`. Setting a client's base URL to `https://host/wl2` makes `/wl2/index.php/api/v2/...` reach it. `/wl2/api/v2/lookup` is also exposed as an alias for grayline.
- Branching on the Host subdomain is possible with axum but not adopted.
- A compatibility token is fixed to one Logbook when issued. Tokens for grayline are issued with the `wl2_` prefix.
- Each Station keeps its old Wavelog integer ID as a mapping, so existing clients keep their `station_id` setting unchanged.
- Only what the local clients depend on is matched: `station_profile_id`, `since_id` / `lastfetchedid` / `has_more`, `format=adif`, `400 validation_error` with `details.duplicate` on duplicates, `lookup?detail=basic`, and the `contest` and `subdivisions` topics of `catalog`.
- The compatibility `since_id` is fixed to the insertion `seq`. Differential sync of edits and deletions is enabled for clients that have moved to the native API.
- How to handle legacy `POST`s without a UUID is left undecided for now. The clients (hislog, speech-to-qso) are fixed later.

### ADIF Input and Output

- Lengths are codepoints by default; bytes can be chosen as an export option. qso-tools is given `--lenient codepoints`.
- Reading uses adif-reader (github.com/kb10uy/adif-reader).
- The writer is to be added on the adif-reader side. Its placement (the same crate or a neighboring one) is open.
- No shared crate is split out; for now everything stays within this repository. Replacing hislog / qso-tools / grayline is a separate task after things stabilize.
- The QSL state of record is the ADIF fields (`QSL_SENT` / `QSL_RCVD` / `*_VIA` / `QSLSDATE` / `QSLRDATE`, `EQSL_QSL_*`, `LOTW_QSL_*`). No separate set of booleans is kept.
- qsl_required is `QSL_SENT in {R, Q}`; received is `QSL_RCVD in {Y, V}`.
- An export records the target UUIDs and `revision`s as a batch, and after the external processing only the matching rows of that batch are updated.

### Delivery and Sync

- Use SSE. Do not use WebSocket.
- The event `id` is `updated_seq`. Reconnection uses `Last-Event-ID`; after a page reload the client passes its stored cursor as `?after=`.
- Differences are not narrowed by filters; the whole Logbook is streamed and the client re-evaluates its display conditions against the current rows.
- The initial sync takes the maximum `updated_seq` at start as H, fetches everything, then reads `after H`. Rows that arrive twice are upserts and therefore harmless.
- Attach `keep_alive` to axum's `Sse`.
- After restoring a backup, change the cursor generation of every Logbook to demand a full resync. A client whose stored generation differs discards its cursor and syncs from the start.

### Position of the Clients

- speech-to-qso: absorbed into the web UI. The voice entry screen becomes one screen of the PWA, and the offline queue is shared with Live entry.
- hislog: moves to the native API after implementing a persistent unsent queue with UUIDs.
- grayline: lookup compatibility for the time being. Moving to the native API is low priority.
- wavelog-tools: export / subdivisions compatibility for the time being. Moves once the native CLI is complete.

### Reference Data

- Updates are done with CLI subcommands (fetch, verify, switch versions). The web UI only shows the version.
- DXCC and zones already recorded on existing QSOs are not changed automatically by a reference data update.
- JCX is handled as a string, keeping leading zeros.

### Migrations and Deployment

- Migrations are embedded in the binary with `sqlx::migrate!()` and applied forward-only. `serve` applies the pending ones at startup; a `migrate` subcommand applies them explicitly, and an environment variable turns the automatic run off for a deployment that wants the explicit step.
- sqlx records each applied migration with its checksum in `_sqlx_migrations`, so an edited applied file stops the server at startup instead of running.
- Before applying pending migrations, the server snapshots the database with `VACUUM INTO` under the data directory. There is no downgrade; rolling back means restoring that snapshot.
- One process owns the database file, so there is no separate migration job or init container. A deployment is one container holding the binary and `client/build`, with the data directory (database, reference data, snapshots) on a volume and TLS terminated outside.
- Query macros that need the schema at compile time keep their `cargo sqlx prepare` output in the repository, refreshed whenever a migration is added.

## Open Questions

- The fields QSL merge applies automatically, and the handling of ambiguous matches (at import implementation)
- How much of the backup and restore procedure the first version includes
- Where the ADIF writer lives
- The crate structure

## Implementation Order (Draft)

1. Contracts and fixtures: IDs and resends, settling the own station information, Japanese text and lengths in ADIF, QSL values, compatibility responses, the permission table.
2. Core and CLI: Logbook / Membership / Station / QSO, setup, import preview and apply, export, backup.
3. Read compatibility and the web UI: station / lookup / catalog / ADIF, log list and editing, Live entry and voice entry, SSE, bulk QSL changes.
4. Reworking the writing clients: hislog's persistent UUID queue, absorbing speech-to-qso.
5. Migration rehearsal and cutover: export Wavelog's ADIF per Station and import it, then compare the ADIF for qcgen / loggen / POTA.
