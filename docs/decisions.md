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
- Heuristic duplicate detection (same call, same band, same mode, compared ignoring case, and `TIME_ON` within 15 minutes) never refuses a record and only warns. It is returned both by a dedicated endpoint (checking a candidate during entry) and in the `warnings` of the `POST` response, with the detection shared between them.
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
- Because values are kept as they arrived, ADIF enumerations (`BAND`, `MODE`, `QSL_SENT`, `QSL_RCVD` and so on) are compared ignoring ASCII case.
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
- Disabling or demoting the last active admin is refused.
- A Logbook keeps at least one active owner: disabling the only active owner of a Logbook is refused until another owner is set, and so is removing or demoting the last active owner.
- Disabling a user deletes their sessions, API tokens and unused registration tokens. Enabling them again restores none of these. Changing a user's callsign is not in the first version.
- Deleting a Logbook is an admin operation that cascades to everything it owns, its rescue records included; the CLI confirms and snapshots the database first.
- An admin sets an owner of a Logbook as a rescue with a reason. The existing owners stay owners.
- Instance settings are written by an admin; the server reads them without an access.
- Reference data versions are written by an admin (the CLI) and read by any signed-in user.

### Sign-in and Credentials

- Sessions, API tokens and registration tokens are random 256-bit values; only their SHA-256 is stored. A `wl2` token for grayline carries the `wl2_` prefix as part of its plaintext.
- A session lasts 30 days and is extended on use: once its expiry is more than a day old, it is pushed to 30 days from now. Signing out deletes it.
- A token's kind decides which API it opens: a `native` token works only on the native API, a `wl2` token only under `/wl2`.
- Issuing a token requires a Membership in every Logbook of its scopes. A token is revoked by its user or by an admin; an owner cannot revoke the tokens of the Logbook's members.
- A token's `last_used_at` is written at most once a day, so that reads do not become writes.
- A registration token is valid for 24 hours unless the CLI gives another period. Issuing one for a user invalidates that user's unused ones.
- A registration link is `PUBLIC_ORIGIN/register#<token>`. The token is in the fragment, so it never reaches the server's request logs; the client sends it in the registration request.
- Re-registration only adds a passkey. A lost passkey is removed with a separate deletion. A user cannot delete their own last passkey; an admin can delete another user's last one.
- WebAuthn ceremony state is kept in memory, as in speech-to-qso. The store keeps the `Passkey` as JSON and does not depend on webauthn-rs.

### Bulk Changes

- A bulk QSL change takes QSO IDs, each with the revision it is based on, and assignments to the QSL columns only. Values are checked against the ADIF enumerations ignoring case and dates as `YYYYMMDD`, and stored as given. Rows that are missing, deleted or at another revision are skipped and reported; the others are updated. Operators can do it.
- Applying an export batch is a bulk QSL change over the batch rows at their recorded revisions, dated with the UTC day of the application: `qsl_card` sets `QSL_SENT = Y` and `QSLSDATE`, `eqsl` sets `EQSL_QSL_SENT = Y` and `EQSL_QSLSDATE`, and `lotw` sets `LOTW_QSL_SENT = Y` and `LOTW_QSLSDATE`. `QSL_SENT_VIA` is not written. `adif` and `pota` batches are records only and cannot be applied. A batch is applied once, in one transaction.
- Rewriting the past QSOs of a Station is reserved for owners. It covers the Station's QSOs, optionally within a QSO time range, and overwrites `STATION_CALLSIGN` and the keys present in the Station's fields, leaving the other fields alone; rows that already match are not touched. A preview returns the number of rows; applying takes that number and refuses with a conflict if it no longer matches.

### Search

- The client keeps the whole Logbook and filters locally, so the server's search is the minimum the APIs, the CLI and exports need: a QSO time range, the Station, band and mode ignoring case, the call exactly (ignoring case) or by `call_base`, a set of `QSL_SENT` values in which an empty value matches an absent field, and `contest_id`. Deleted rows are excluded. Results are newest first with keyset paging on `(qso_time, id)`. There is no substring search.

### Store and Authorization Boundaries

- There are no Repository traits and no separate service layer. Each operation (creating a QSO, archiving a Station and so on) is one concrete function in `server/src/store/`, shared by the native API, the `/wl2` layer and the CLI. Smaller SQL helpers inside the store take a plain connection and are not the entry points. A trait or a service layer is added only when it removes a concrete duplication.
- Every operation that writes runs in a write transaction opened with `BEGIN IMMEDIATE`, so the write lock is taken before the first read and a later write never fails with a busy error that `busy_timeout` cannot wait out. The transaction is a type only the store can create, and every public write function requires it; a caller cannot forget to open one or open a deferred one.
- The unit of atomicity is one operation: numbering, checking and saving one QSO happen in one transaction. Bulk work such as an import calls the same operation in batches, a bounded number per transaction, so write transactions stay short. A rerun of an interrupted import is idempotent through IDs derived from the records (see ADIF Import).
- Authorization is a value. The store checks a principal (a signed-in user, or an API token with its user and Logbook scopes) against the user's state, the token's scopes and the Membership, and only then hands out a Logbook access that carries the Logbook, the user and the role. Logbook operations take the Logbook from that access rather than from a separate argument, so an access for one Logbook cannot be used on another.
- Access values retain their principal and are revalidated at each store operation. Writes revalidate inside their write transaction, including the current user state, token scopes and required role; an old access cannot bypass removal, demotion or revocation. QSO creation uses the user's current callsign for the default `OPERATOR`.
- Operations reserved for owners (Stations, Memberships) require an owner access, which can only be obtained from a Logbook access whose role is `owner`. Instance operations (users, Logbooks) require an admin access, which only a signed-in admin obtains; API tokens never do.
- An admin outside a Logbook obtains a Logbook access only through a rescue, which records the actor, the action and the reason in `rescue_operations` in the same transaction as the operation.
- A rescue access is valid only while its issuing write transaction is active and its uncommitted audit row is visible on the operation's connection. Commit or rollback invalidates it, and it cannot be used from another connection. Each rescue operation also revalidates the admin's current authority.
- The authorization lives in the store, not in the web layer. The CLI acts as a named user and goes through the same checks, which also gives `created_by` and `updated_by` a user. The user is named by callsign with `--as` (or `CLI_CALLSIGN`); the CLI is not authenticated, since whoever runs it can already read the database file.
- A Logbook or a row the principal cannot access is reported as not found, the same as one that does not exist.

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
- The writer is to be added on the adif-reader side. Its placement (the same crate or a neighboring one) is open; until it is settled the writer lives in `server/src/domain/adif.rs`, and its expected output is in `fixtures/adif/` for the client tests to share.
- No shared crate is split out; for now everything stays within this repository. Replacing hislog / qso-tools / grayline is a separate task after things stabilize.
- The QSL state of record is the ADIF fields (`QSL_SENT` / `QSL_RCVD` / `*_VIA` / `QSLSDATE` / `QSLRDATE`, `EQSL_QSL_*`, `LOTW_QSL_*`). No separate set of booleans is kept.
- qsl_required is `QSL_SENT in {R, Q}`; received is `QSL_RCVD in {Y, V}`.
- An export records the target UUIDs and `revision`s as a batch, and after the external processing only the matching rows of that batch are updated.

### ADIF Import

- An import either adds new QSOs or merges QSL confirmations into existing ones. The CLI is told which by the source (`new`, `lotw`, `eqsl` or `card`); it is never guessed from the file.
- Without `--apply` an import only previews: it reads, checks and matches every record and reports what applying would do, writing nothing. With `--apply` it writes, a bounded number of records per write transaction.
- A new QSO's ID is derived from its record: a UUID v7 whose timestamp is the QSO time and whose remaining bits come from the SHA-256 of the Logbook ID, the record's fields as read (names upper-cased and sorted, values as they arrived) and the number of identical records before it in the file. A rerun of an interrupted or repeated import meets the rows it already created and the idempotent create returns them; a deleted row stays deleted. Editing some records and rerunning adds only those records.
- The timestamp of such an ID is the QSO time rather than the time of entry. Nothing relies on it: insertion order is `seq`.
- Records that cannot be read or checked are skipped and reported by their position; the others go ahead. A possible duplicate of an existing row is a warning, as on entry.
- A QSL merge writes only the fields of its source, and only towards received:

  | Source | Confirmation in the record | Fields written |
  | --- | --- | --- |
  | `lotw` | `LOTW_QSL_RCVD`, or else `QSL_RCVD` (as in `lotwreport.adi`), is `Y`; the date is `LOTW_QSLRDATE`, or else `QSLRDATE` | `LOTW_QSL_RCVD = Y`, `LOTW_QSLRDATE` |
  | `eqsl` | `EQSL_QSL_RCVD`, or else `QSL_RCVD`, is `Y`; a record with neither is a confirmation (an eQSL inbox); the date is `EQSL_QSLRDATE`, or else `QSLRDATE` | `EQSL_QSL_RCVD = Y`, `EQSL_QSLRDATE` |
  | `card` | `QSL_RCVD` is `Y` or `V`; the date is `QSLRDATE` | `QSL_RCVD` as given, `QSLRDATE`, and `QSL_RCVD_VIA` when the record has it |

- A confirmation without a date is dated with the UTC day of the import. A QSO whose received field for the source is already `Y` or `V` is left alone, and nothing is set back to `N`.
- Other fields a confirmation carries (`DXCC`, `CQZ`, `ITUZ`, `STATE`, `CNTY`, `GRIDSQUARE`, `IOTA` in LoTW's report) are never applied; the preview counts the QSOs where they differ.
- A record matches the live QSOs of the Logbook, or of one Station when it is given, with the same call ignoring case, the same band (from `FREQ` when the record has no `BAND`), a mode where the `MODE` or `SUBMODE` of one equals the `MODE` or `SUBMODE` of the other ignoring case, `TIME_ON` within 15 minutes, and the same `STATION_CALLSIGN` when the record has one.
- Exactly one match is merged. No match is reported as unmatched. Several matches are ambiguous and never merged unless all of them are already received; they are resolved by hand. Unmatched, ambiguous and invalid records can be written to a rejects file as ADIF. Two records matching the same QSO merge the first only.
- Applying is a bulk QSL change at the revisions read while matching, so a row changed in between is skipped and reported. A rerun finds the merged rows already received and changes nothing.

### Backup and Restore

- `backup` writes a `VACUUM INTO` copy of the database while the server runs, to a timestamped file in the `backups` directory next to the database or to a given path. `--keep N` then removes all but the newest N backups in that directory. Scheduling and off-site copies are left to the host (cron, a systemd timer).
- A backup is the database only. Reference data is fetched again with the CLI; the configuration and the environment belong to the deployment.
- `serve` holds an exclusive lock on a `.lock` file next to the database. `restore` takes the same lock, so it refuses while the server runs and the server cannot start during a restore.
- `restore` copies the backup next to the database and checks the copy: `PRAGMA integrity_check`, and every migration applied in it known to this binary with the same checksum. It then snapshots the current database to `snapshots`, swaps the copy in, applies pending migrations and, in one transaction, replaces the cursor generation of every Logbook and deletes every session and every unused registration link. The backup file itself is never modified.
- Sessions and registration links are deleted because the backup may hold some that were revoked after it was taken; everyone signs in again. API tokens revoked after the backup come back too, so the restore lists the tokens it restored for an admin to review.
- Restoring from the web UI, scheduled backups inside the server, continuous replication and restoring a single Logbook are not in the first version; a single Logbook travels as an ADIF export.

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

- Where the ADIF writer lives
- The crate structure

## Implementation Order (Draft)

1. Contracts and fixtures: IDs and resends, settling the own station information, Japanese text and lengths in ADIF, QSL values, compatibility responses, the permission table.
2. Core and CLI: Logbook / Membership / Station / QSO, setup, import preview and apply, export, backup.
3. Read compatibility and the web UI: station / lookup / catalog / ADIF, log list and editing, Live entry and voice entry, SSE, bulk QSL changes.
4. Reworking the writing clients: hislog's persistent UUID queue, absorbing speech-to-qso.
5. Migration rehearsal and cutover: export Wavelog's ADIF per Station and import it, then compare the ADIF for qcgen / loggen / POTA.
