# Design Decisions

The design of jl1his-logger, the QSO log of record for JL1HIS, and the reasons behind it, as implemented on 2026-10-25. A change that departs from a decision here, or settles something left open, updates this file in the same change.

## Background

- The log is the single source of truth for contacts. QSOs are entered in the web UI or sent by logging clients through an API, then exported as ADIF and fed to eQSL, LoTW, contest summary sheets, QSL card generation and POTA.
- The application does only what this use needs, with a web UI built for entering and checking QSOs.
- Logging clients written for Wavelog API v2 keep working through a compatibility layer that serves the four resources they use: `qso`, `station`, `lookup` and `catalog`.

## Structure

- A new, separate repository (this one) holding one server, one SQLite database and a same-origin SPA / PWA.
- The server is Rust + axum; the client is SvelteKit.
- One SQLite database. Set WAL and `busy_timeout`, and keep write transactions short.
- The server is one package: no workspace. `domain` stays free of the database, network and file system by convention and review. It is split into a crate of its own only when another binary or repository needs it, or when build times call for it.
- The query syntax of Search is the one shared crate, `query/` beside `server/`: a path dependency of the server, without a workspace, and compiled to WASM with `wasm-bindgen` (its `wasm` feature) for the client. It holds parsing a query and matching a row, with the `call_base` rule matching needs, and nothing else; the server turns the parsed terms into SQL. It has no dependencies outside the `wasm` feature, so no `regex`, and the module is about 70 KB. The client builds it with `wasm-pack` before it builds itself, and loads it asynchronously before the first query.

## Required Features

- QSO store, Station Location, Instrument, Logbook, User, Membership, API tokens.
- Write API (create, update, delete), read API (filters and ADIF output), differential fetch, SSE.
- ADIF import (loading existing logs, and merging QSL fields into existing QSOs).
- Confirmation state (paper QSL, eQSL and LoTW held separately).
- Reference data: cty.dat, frequencies and bands, modes and submodes, subdivisions, POTA parks.
- lookup: entity information from cty.dat and previous contacts in the own log (name / grid / QTH).
- Web UI: log list and search, single-QSO editing, Live QSO entry, voice entry, Station Location and Instrument editing, ADIF import and export, bulk QSL state changes.

## Out of Scope

Dashboards, maps, DX cluster, QRZ / HamQTH integration, CAT, contest modules, awards, satellites, themes, organization hierarchies and fine-grained permissions.

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
- When a QSO is created through the native API or `/wl2`, the server fills the `DXCC`, `CQZ`, `ITUZ` and `CONT` the record lacks from `cty.dat`, `DXCC` needing the `dxcc` table as well. Values given are never overwritten, nothing is filled for a kind that is not installed, and editing a QSO later does not fill them again. Reference data that cannot be read is logged and never refuses the QSO. On `/wl2` the fill comes after the ID is derived, so a resend keeps its ID across reference data updates. An ADIF import keeps its records as they arrived.
- A Station holds no equipment: `MY_RIG`, `MY_ANTENNA` and `TX_PWR` are refused in its fields and belong to Instruments. A migration removes them from the fields of existing Stations; QSOs keep the values already baked into them.

### Instruments

- An Instrument is the equipment of a QSO: a rig, an antenna and a default TX power. It sits above the Station and is independent of it, so any Instrument can be used with any Station of the Logbook. A QSO may have none, and then nothing fills `MY_RIG`, `MY_ANTENNA` or `TX_PWR`.
- A Logbook owns its Instruments, as it owns its Stations. A rig that several Logbooks use is registered in each of them.
- An Instrument has a name, the fixed fields `MY_RIG`, `MY_ANTENNA` and `TX_PWR`, each optional, and a map of other fields like a Station's. The map refuses the fixed fields and `STATION_CALLSIGN`; it may hold keys a Station also holds, and the Instrument's value then wins. A rig whose power or antenna differs by band is registered as one Instrument per band.
- `TX_PWR` is a decimal number of watts greater than zero, with at most six decimal places (`0.5`, `5`, `100`), so QRP and QRPP powers can be written; anything else is refused. It is stored as given.
- The `_INTL` fields are not used. ADIF can write them only in ADX, so `MY_RIG` and `MY_ANTENNA` carry text outside ASCII as the other fields already do.
- When a QSO is created, the given fields win, then the Instrument's values, then the Station's, then the default `OPERATOR`; each fills only what is still absent. `instrument_id` is held on the QSO next to `station_id` for classification and search, and is `NULL` without an Instrument.
- Editing an Instrument does not change past QSOs, and changing a QSO's Instrument does not bake its values in again. Past rows are corrected with an explicit rewrite that states the number of affected rows (see Bulk Changes).
- A Logbook may name an area code field: the ADIF field that holds the regional code of the other station, with the label the screens show for it (`area_field` and `area_label` on the Logbook; `CNTY` labelled "JCC / JCG" in Japan, `CNTY` labelled "County" in the US, `STATE`, or none). Owners set it with `PUT /api/logbooks/{id}/settings` (`{ "area_field": { "name", "label" } | null }`). The name is any ADIF QSO field or `APP_*` field except the required fields (`CALL`, `QSO_DATE`, `TIME_ON`, `BAND`, `MODE`), `STATION_CALLSIGN`, `OPERATOR` and `MY_*`; the client also keeps the fields that have their own place on the screens out of it. Changing the field moves no recorded values. Nothing is Japan-specific by default: a new Logbook has no area code field, and JCC / JCG is a setting.
- Instruments are archived rather than deleted. As for Stations, archiving only takes an Instrument out of the client's choices.
- The CLI lists, creates, edits and archives the Instruments of a Logbook, as it does Stations.

### Favorites

- Each member marks Stations and Instruments of a Logbook as favorites of their own. A favorite is a preference of one User in one Logbook, not a property of the Station or Instrument: it is a row of `station_favorites` or `instrument_favorites`, marking one leaves `updated_at` alone, and other members do not see it.
- Every member may mark any Station or Instrument, though only owners edit Stations. An API token marks and sees the favorites of the User it belongs to, so a desktop entry client lists them as its user's web client does. Removing a Membership removes that member's favorites.
- `GET /stations` and `GET /instruments` return `favorite` for the caller and list favorites first among the active ones, then the others by name, then the archived ones. An archived favorite stays a favorite and comes back as one when it is restored.
- `/wl2` does not show favorites; its `station` list keeps the order of Wavelog IDs. The CLI marks and lists them for the user it acts as, with `station favorite` / `unfavorite` and `instrument favorite` / `unfavorite`.

### Storage Shape

- Core fields (call, band, mode, date and time, station, QSL state and so on) are columns; the rest is a JSON string map.
- A field that has a column is not duplicated in the JSON. Unknown standard fields and `APP_*` are not dropped.
- Values are kept as their ADIF strings; normalized forms for search (the normalized callsign, time in seconds, frequency in Hz) go to derived columns.
- Because values are kept as they arrived, ADIF enumerations (`BAND`, `MODE`, `QSL_SENT`, `QSL_RCVD` and so on) are compared ignoring ASCII case.
- Composite foreign keys enforce that a QSO belongs to the same Logbook as its Station and its Instrument, and that an export batch contains only QSOs from its own Logbook.
- The tables are described in [schema.md](schema.md).

### FREQ and Parks

- A QSO without FREQ can be saved. The server, imports and the `/wl2` layer never fill in a representative frequency.
- In the web client's editors (live entry, voice entry, QSO editing), choosing a band sets the frequency field to that band's representative frequency, overwriting what was there, and typing a frequency moves the band to the one containing it. The representative frequencies are a client table next to the band table, each inside its band (for example 7.000, 14.000, 18.100, 144.000 and 430.000 MHz). A frequency outside every band leaves the band alone.
- Several parks are one row in the store, with several references in `MY_POTA_REF`; the POTA submission export projects one record per park with `MY_SIG_INFO` expanded. QSOs are not duplicated.

### Users and Permissions

- Instance roles (held by a User): `admin` / `user`.
- Logbook roles (held by a Membership): `owner` / `operator`.
- A User is a person. Signs in with a passkey. Has a personal callsign.
- A Logbook is the unit of a personal or club station and owns its Station Locations and Instruments.
- A QSO's `OPERATOR` defaults to the callsign of the person who entered it, and `created_by` / `updated_by` record the User.
- `Membership.owner` is an application administration right, separate from ADIF's `OWNER_CALLSIGN`.
- An admin's responsibilities lie outside Logbooks: creating and disabling users, invitation links, passkey re-registration, creating and deleting Logbooks and reassigning their owner, revoking other people's tokens, reference data updates, initial setup, backups, and instance settings.
- An owner manages the Stations and Memberships of its own Logbook but cannot create Users.
- Even an admin needs a Membership to read QSOs. A rescue operation is explicit and records its reason and actor.
- An operator can edit and delete QSOs entered by others.
- Unlike Stations, Instruments are managed by every member: an operator creates, edits, archives and restores any Instrument of the Logbook, so a member who brings a rig to a club station registers it. Rewriting the past QSOs of an Instrument stays with owners.
- API tokens belong to a User and are limited to Logbooks by scopes. The plaintext is shown only when issued.
- The first admin is created by a CLI setup command, which issues a one-time registration URL.
- Disabling or demoting the last active admin is refused.
- A Logbook keeps at least one active owner: disabling the only active owner of a Logbook is refused until another owner is set, and so is removing or demoting the last active owner.
- Disabling a user deletes their sessions, API tokens and unused registration tokens. Enabling them again restores none of these. Changing a user's callsign is not in the first version.
- Deleting a Logbook is an admin operation that cascades to everything it owns, its rescue records included; the CLI confirms and snapshots the database first.
- An admin sets an owner of a Logbook as a rescue with a reason. The existing owners stay owners.
- Instance settings are written by an admin; the server reads them without an access. The CLI has generic `settings get`, `set` and `list` over free keys; each key is defined by the feature that reads it.
- Reference data versions are written by an admin (the CLI) and read by any signed-in user.

### Sign-in and Credentials

- `public_origin` requires a DNS hostname for WebAuthn. IP addresses are refused during configuration validation; local development uses `http://localhost`.
- Sessions, API tokens and registration tokens are random 256-bit values; only their SHA-256 is stored. A `wl2` token issued with the `wl2_` prefix carries it as part of its plaintext.
- A session lasts 30 days and is extended on use: at most once a day, its use is recorded and its expiry pushed to 30 days from now, but never past 90 days from its creation; after that the user signs in again. Signing out deletes it. A session records the `User-Agent` it was created with (at most 256 characters), so a user can tell their sessions apart, list them and revoke any of them; an admin can do the same for another user.
- A token's kind decides which API it opens: a `native` token works only on the native API, a `wl2` token only under `/wl2`.
- Issuing a token requires a Membership in every Logbook of its scopes. A token is revoked by its user or by an admin; an owner cannot revoke the tokens of the Logbook's members.
- A token's `last_used_at` is written at most once a day, so that reads do not become writes.
- A registration token is valid for 24 hours unless the CLI gives another period. Issuing one for a user invalidates that user's unused ones.
- A registration link is `public_origin/register#<token>`. The token is in the fragment, so it never reaches the server's request logs; the client sends it in the registration request.
- Re-registration only adds a passkey. A lost passkey is removed with a separate deletion. A user cannot delete their own last passkey; an admin can delete another user's last one.
- WebAuthn ceremony state is kept in memory. The store keeps the `Passkey` as JSON and does not depend on webauthn-rs.
- A ceremony is used once and expires after 5 minutes. At most 10 000 are in flight; one started past that evicts the oldest, so a flood of unauthenticated sign-in starts slows only itself down instead of refusing everyone with `429`. Request rate limits are the reverse proxy's. A restart drops the ceremonies and the browser starts again.
- Passkeys are discoverable credentials: registration requires a resident key and user verification, and sign-in asks for no callsign. The WebAuthn user handle is the user ID; a passkey whose row belongs to another user than its handle, or to a disabled user, does not sign in.
- Reading and verifying a passkey, updating its signature counter and creating a session run in one write transaction, so concurrent sign-ins cannot accept a stale counter.
- Completing a registration link consumes it and stores the passkey in one transaction, then signs the user in. A signed-in user adds further passkeys without a link.
- The session cookie is `__Host-session` with `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/` and a 30-day `Max-Age`, which `GET /api/me` renews.
- A state-changing request (any method but `GET`, `HEAD` and `OPTIONS`) is refused when it carries an `Origin` other than `public_origin`, or a `Sec-Fetch-Site` other than `same-origin` or `none`. A request with neither header is not from a browser and carries no ambient cookie, so API token clients need not send them.
- Every response carries `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: same-origin`, a `Permissions-Policy` denying camera, geolocation and payment, and the `Content-Security-Policy` `frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'`. Responses under `/api` and `/wl2` are `Cache-Control: no-store` unless the handler sets its own. The script, style and connect directives belong to the client, which emits them with hashes through SvelteKit's `kit.csp`; the two policies combine. HSTS is the TLS-terminating proxy's.

### Bulk Changes

- A bulk QSL change takes QSO IDs, each with the revision it is based on, and assignments to the QSL columns only. Values are checked against the ADIF enumerations ignoring case and dates as `YYYYMMDD`, and stored as given. Rows that are missing, deleted or at another revision are skipped and reported; the others are updated. Operators can do it.
- Applying an export batch is a bulk QSL change over the batch rows at their recorded revisions, dated with the UTC day of the application: `qsl_card` sets `QSL_SENT = Y` and `QSLSDATE`, `eqsl` sets `EQSL_QSL_SENT = Y` and `EQSL_QSLSDATE`, and `lotw` sets `LOTW_QSL_SENT = Y` and `LOTW_QSLSDATE`. `QSL_SENT_VIA` is not written. `adif` and `pota` batches are records only and cannot be applied. A batch is applied once, in one transaction.
- Rewriting the past QSOs of a Station is reserved for owners. It covers the Station's QSOs, optionally within a QSO time range, and overwrites `STATION_CALLSIGN` and the keys present in the Station's fields, leaving the other fields alone; rows that already match are not touched. A preview returns the number of rows; applying takes that number and refuses with a conflict if it no longer matches.
- Rewriting the past QSOs of an Instrument works the same way and is reserved for owners too. It covers the Instrument's QSOs, optionally within a QSO time range, and overwrites the fixed fields the Instrument has a value for and the keys present in its fields.

### Search

- The client keeps no copy of the Logbook. The log list sends its query to the server and reads the matches newest first, with keyset paging on `(qso_time, id)`, loading the next page as the end of the list comes into view. The first page carries the total number of matches. Deleted rows are excluded.
- The query is one text syntax, shared by the web UI, the native API (`q` on the QSO search, open to API tokens from the start) and the filters of export batches; the CLI is to take it as well (see Remaining Work). It is a list of terms separated by spaces, all of which must hold. A comma-separated value matches any of its parts, and a value containing spaces is quoted:

  | Term | Holds when |
  | --- | --- |
  | `call:JA1*` | The call matches, ignoring case, with `*` and `?` as wildcards |
  | `base:JA1XYZ` | `call_base` equals the value, ignoring case |
  | `band:40m,20m`, `mode:SSB`, `contest:ALLJA1` | `BAND`, `MODE` or `CONTEST_ID` is one of the values, ignoring case |
  | `sent:R,Q`, `rcvd:Y,V` | `QSL_SENT` or `QSL_RCVD` is one of the values, ignoring case |
  | `lotw-sent:Y`, `lotw-rcvd:Y`, `eqsl-sent:Y`, `eqsl-rcvd:Y,V` | `LOTW_QSL_SENT`, `LOTW_QSL_RCVD`, `EQSL_QSL_SENT` or `EQSL_QSL_RCVD` is one of the values, ignoring case |
  | `station:Takao` | The Station's name or the QSO's `STATION_CALLSIGN` contains the value, ignoring case |
  | `instrument:IC-705` | The Instrument's name or the QSO's `MY_RIG` contains the value, ignoring case |
  | `park:JP-0012` | One of the references in `MY_POTA_REF` is the value |
  | `dxcc:339` | `DXCC` is one of the entity codes |
  | `date:2026-10-01..2026-10-19`, `date:>=2026-10-18` | The UTC date of the QSO time is in the range (either end may be open) or compares with `>`, `>=`, `<`, `<=` |
  | `freq:14.0..14.35`, `freq:>=50` | `FREQ` in MHz is the value, is in the range (either end may be open, both ends included) or compares with `>`, `>=`, `<`, `<=`; a QSO without `FREQ` never holds |
  | `pwr:<=5`, `pwr:10..50` | `TX_PWR` in watts compares as `freq:` does; a QSO whose `TX_PWR` is missing or not a decimal number never holds |
  | `has:freq`, `no:freq` | The ADIF field, by name ignoring case, is present or absent; an empty value counts as absent. `park`, `sent`, `rcvd`, `contest`, `lotw-sent`, `lotw-rcvd`, `eqsl-sent`, `eqsl-rcvd` and `pwr` name `MY_POTA_REF`, `QSL_SENT`, `QSL_RCVD`, `CONTEST_ID`, `LOTW_QSL_SENT`, `LOTW_QSL_RCVD`, `EQSL_QSL_SENT`, `EQSL_QSL_RCVD` and `TX_PWR` |
  | `adif.GRIDSQUARE:PM95` | The ADIF field, by name ignoring case, is one of the values, ignoring ASCII case |
  | `yokohama` | The call, `NAME`, `QTH` or `COMMENT` contains the word, ignoring case |
  | `-term` | The term does not hold |

- Values are compared as their ADIF strings, enumerations ignoring ASCII case (see Storage Shape). `freq:` and `pwr:` are the exceptions: both sides are read as decimal numbers to six places, exactly, so `freq:14.074` matches `14.0740`; the server compares `freq:` on the derived `freq_hz`. Wildcard and substring terms scan the Logbook's rows; at the size of one station's log no index or full-text table is needed, and FTS5 is added only when that stops being true.
- An unknown field, a bad date or number or a malformed term is `400`, naming the term and its position so the client can mark it, with a code the client words it from (see Localization). A term is never dropped silently.
- Parsing and matching are one implementation, the query crate (see Structure). The server translates each term into SQL, and a store test runs a set of queries both through SQL and through the crate's matcher over the same rows, so the two cannot drift apart. The client uses the crate to show the terms as chips while typing (an unfinished term such as `call:` is shown as pending and the query is not sent) and to test the rows SSE delivers against the query.
- The structured filters of the first version (a QSO time range, the Station, band, mode, the call or `call_base`, a set of `QSL_SENT` values in which an empty value matches an absent field, and `contest_id`) stay as parameters and combine with `q`.
- Selecting every match in the log list asks the server for the IDs and revisions of all matches (`GET /targets` with the same filters), and a bulk change takes that list (see Bulk Changes). A bulk change is never applied to a query directly, so what changes is what was selected, at the revisions it was selected at.

### Store and Authorization Boundaries

- There are no Repository traits and no separate service layer. Each operation (creating a QSO, archiving a Station and so on) is one concrete function in `server/src/store/`, shared by the native API, the `/wl2` layer and the CLI. Smaller SQL helpers inside the store take a plain connection and are not the entry points. A trait or a service layer is added only when it removes a concrete duplication.
- Every operation that writes runs in a write transaction opened with `BEGIN IMMEDIATE`, so the write lock is taken before the first read and a later write never fails with a busy error that `busy_timeout` cannot wait out. The transaction is a type only the store can create, and every public write function requires it; a caller cannot forget to open one or open a deferred one.
- The unit of atomicity is one operation: numbering, checking and saving one QSO happen in one transaction. Bulk work such as an import calls the same operation in batches, a bounded number per transaction, so write transactions stay short. A rerun of an interrupted import is idempotent through IDs derived from the records (see ADIF Import).
- Authorization is a value. The store checks a principal (a signed-in user, or an API token with its user and Logbook scopes) against the user's state, the token's scopes and the Membership, and only then hands out a Logbook access that carries the Logbook, the user and the role. Logbook operations take the Logbook from that access rather than from a separate argument, so an access for one Logbook cannot be used on another.
- Access values retain their principal and are revalidated at each store operation. Writes revalidate inside their write transaction, including the current user state, token scopes and required role; an old access cannot bypass removal, demotion or revocation. QSO creation uses the user's current callsign for the default `OPERATOR`.
- Operations reserved for owners (Stations, Memberships, rewriting the QSOs of an Instrument) require an owner access, which can only be obtained from a Logbook access whose role is `owner`. Instance operations (users, Logbooks) require an admin access, which only a signed-in admin obtains; API tokens never do.
- An admin outside a Logbook obtains a Logbook access only through a rescue, which records the actor, the action and the reason in `rescue_operations` in the same transaction as the operation.
- A rescue access is valid only while its issuing write transaction is active and its uncommitted audit row is visible on the operation's connection. Commit or rollback invalidates it, and it cannot be used from another connection. Each rescue operation also revalidates the admin's current authority.
- The authorization lives in the store, not in the web layer. The CLI acts as a named user and goes through the same checks, which also gives `created_by` and `updated_by` a user. The user is named by callsign with `--as`; the CLI is not authenticated, since whoever runs it can already read the database file.
- A Logbook or a row the principal cannot access is reported as not found, the same as one that does not exist.

### Wavelog API v2 Compatibility Layer

- Mounted under the path prefix `/wl2`. Setting a client's base URL to `https://host/wl2` makes `/wl2/index.php/api/v2/...` reach it. `/wl2/api/v2/lookup` is also exposed as an alias.
- Branching on the Host subdomain is possible with axum but not adopted.
- A compatibility token is fixed to one Logbook when issued. It can be issued with the `wl2_` prefix, by which some clients choose API v2.
- A Station may carry a Wavelog integer ID, unique within its Logbook, by which `/wl2` clients address it, so a client keeps the `station_id` it is configured with.
- Instruments have no place in `/wl2`: a QSO logged there has none, and the `power` of the `station` list is always `null`, since a Station has no `TX_PWR`. A client that needs an Instrument uses the native API, where creating and replacing a QSO take an optional `instrument_id`.
- Only a subset is matched: `station_profile_id`, `since_id` / `lastfetchedid` / `has_more`, `format=adif`, `400 validation_error` with `details.duplicate` on duplicates, `lookup?detail=basic`, and the `contest` and `subdivisions` topics of `catalog`.
- The compatibility `since_id` is fixed to the insertion `seq`. Differential sync of edits and deletions belongs to the native API.
- A legacy `POST qso` carries no UUID, so its ID is derived from the record as an import does (see ADIF Import): a UUID v7 from the QSO time and the SHA-256 of the Logbook ID, the Station UUID and the record's fields after conversion, as the first occurrence. The Station UUID scopes the ID without depending on mutable Station defaults. A resend of the same body meets the row it created, deleted or not, and is answered `400 validation_error` with `details.duplicate`, so a client resending after a lost answer can count it as sent. A possible duplicate found by the heuristic is still created. The answer's `id` is the row's `seq`.

### ADIF Input and Output

- Lengths are codepoints by default; bytes can be chosen as an export option.
- A field name is what the ADIF specification allows: ASCII letters, digits and underscores, starting with a letter, read ignoring case and stored upper-cased. `EOR` and `EOH` are not field names. A record with another name (a `USERDEF` name with spaces, say) is refused, on entry and in an import.
- A field value is at most 2048 bytes and a QSO has at most 1024 fields outside the columns; the names of passkeys, tokens, Stations and Logbooks are at most 2048 bytes. The bounds are checked where a `Qso` is built and again when it is checked before a write, so nothing larger is stored. A filter lists at most 100 values.
- Reading uses adif-reader (github.com/kb10uy/adif-reader).
- The writer moves into adif-reader as a `write` module of the same crate once the export output has been checked against real QSL card, contest and POTA submissions, so that fixes before then touch one repository. Until then it lives in `server/src/domain/adif.rs`, and its expected output is in `fixtures/adif/` for the client tests to share. Whether the crate is renamed is decided at the move.
- Apart from the query crate (see Structure), no shared crate is split out; for now everything stays within this repository.
- The QSL state of record is the ADIF fields (`QSL_SENT` / `QSL_RCVD` / `*_VIA` / `QSLSDATE` / `QSLRDATE`, `EQSL_QSL_*`, `LOTW_QSL_*`). No separate set of booleans is kept.
- qsl_required is `QSL_SENT in {R, Q}`; received is `QSL_RCVD in {Y, V}`.
- An export records the target UUIDs and `revision`s as a batch, and after the external processing only the matching rows of that batch are updated.

### ADIF Import

- An import either adds new QSOs or merges QSL confirmations into existing ones. The CLI is told which by the source (`new`, `lotw`, `eqsl` or `card`); it is never guessed from the file.
- Without `--apply` an import only previews: it reads, checks and matches every record and reports what applying would do, writing nothing. The preview runs in one read transaction and takes no write lock, so a large preview never blocks entry; duplicate warnings against the earlier records of the file are computed in memory. With `--apply` it writes, a bounded number of records per write transaction.
- A new QSO's ID is derived from its record: a UUID v7 whose timestamp is the QSO time and whose remaining bits come from the SHA-256 of the Logbook ID, the record's fields as read (names upper-cased and sorted, values as they arrived) and the number of identical records before it in the file. A rerun of an interrupted or repeated import meets the rows it already created and the idempotent create returns them; a deleted row stays deleted. Editing some records and rerunning adds only those records.
- A new-QSO import takes an optional Instrument besides its Station. Its values are baked in after the ID is derived, and only where the record has none, so the record's own `MY_RIG`, `MY_ANTENNA` and `TX_PWR` win and choosing another Instrument for a rerun does not change the IDs.
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
- Matching searches every candidate page before deciding whether a match is unique. Exactly one match is merged. No match is reported as unmatched. Several matches are ambiguous and never merged unless all of them are already received; they are resolved by hand. Unmatched, ambiguous and invalid records can be written to a rejects file as ADIF. Two records matching the same QSO merge the first only.
- Applying is a bulk QSL change at the revisions read while matching, so a row changed in between is skipped and reported. A rerun finds the merged rows already received and changes nothing.

### Backup and Restore

- `backup` writes a `VACUUM INTO` copy of the database while the server runs, to a timestamped file in the `backups` directory next to the database or to a given path. `--keep N` then removes all but the newest N backups in that directory. Scheduling and off-site copies are left to the host (cron, a systemd timer).
- A backup is the database only. Reference data is fetched again with the CLI; the configuration belongs to the deployment.
- `serve` holds an exclusive lock on a `.lock` file next to the database. `restore` takes the same lock, so it refuses while the server runs and the server cannot start during a restore.
- `restore` copies the backup next to the database and checks the copy: `PRAGMA integrity_check`, and every migration applied in it known to this binary with the same checksum. It applies pending migrations to the copy and, in one transaction, replaces the cursor generation of every Logbook and deletes every session and every unused registration link. Only after the copy is ready does it snapshot the current database to `snapshots` and replace it with the copy. A preparation or replacement failure leaves the current database in place. The backup file itself is never modified.
- Sessions and registration links are deleted because the backup may hold some that were revoked after it was taken; everyone signs in again. API tokens revoked after the backup come back too, so the restore lists the tokens it restored for an admin to review.
- Restoring from the web UI, scheduled backups inside the server, continuous replication and restoring a single Logbook are not in the first version; a single Logbook travels as an ADIF export.

### Delivery and Sync

- Use SSE. Do not use WebSocket.
- The event `id` is `updated_seq`. Reconnection uses `Last-Event-ID`; after a restart a client that keeps a copy of the Logbook passes its stored cursor as `?after=`. When the browser gives up on a stream (a `401`, `404` or `503`), the web client opens a new one after its last `updated_seq` with `?after=`, waiting 2 seconds and doubling the wait up to a minute until a `hello` arrives.
- Differences are not narrowed by filters; the whole Logbook is streamed and the client re-evaluates its display conditions against the current rows.
- A client that keeps a copy syncs initially by taking the maximum `updated_seq` at start as H, fetching everything, then reading `after H`. Rows that arrive twice are upserts and therefore harmless.
- The web client keeps no copy (see Search) and no cursor across reloads. It opens the stream, which starts at the latest `updated_seq`, then runs its query. An event updates a loaded row in place by its ID. A row that stops matching stays where it is, marked, until the query runs again. A new row that matches is inserted in order without moving the scroll position. A new row outside the query is only counted, in a small "N new outside this query" next to the list. A changed cursor generation reruns the query.
- Attach `keep_alive` to axum's `Sse`.
- A stream opens with a `hello` event carrying the cursor generation and the latest `updated_seq`. Successful state-changing requests wake every stream at once, except those under `/api/auth`, which anyone can call and which would otherwise let an anonymous caller make every stream reread. At most 64 streams are open per server; a further one is refused with `503` and the client retries later. The streams also poll every 10 seconds, which picks up writes the CLI makes from another process and ends the stream of a caller who lost the Logbook or whose session expired or was deleted. Session checks do not renew the session. A stream queues at most one page of 500 QSOs and rechecks authorization on writes and polling deadlines even while sending that queue. Streams discard queued events when the server shuts down, so graceful shutdown does not wait on them.
- After restoring a backup, change the cursor generation of every Logbook to demand a full resync. A client whose stored generation differs discards its cursor and syncs from the start.

### Web Client Routes

- Every screen has a path of its own, so moving between screens adds browser history and a screen can be bookmarked or reloaded. The client routes as a single-page app; the server already answers any other path outside `/api` and `/wl2` with `index.html`.
- Screens of a Logbook carry its ID: `/logbooks/{logbook_id}` (the log, titled `Logbook`), `/entry`, `/voice`, `/qsos/{qso_id}` (editing one QSO), `/stations`, `/stations/{station_id}`, `/instruments`, `/instruments/{instrument_id}`, `/qsl-management`, `/import-export`, `/members`, `/settings` (the Logbook's area code field) and, on mobile, `/more`. Switching the Logbook keeps the screen. Screens of the user or the instance do not: `/settings/account`, `/settings/tokens`, `/settings/voice` (the speech language of this device), `/settings/language` (the display language of this device, see Localization) and `/settings/server` (the server's version and commit and the reference data versions), with `/settings` listing them on mobile. Settings shows the current Logbook's settings and members as two of its sections, at that Logbook's `/settings` and `/members`. `/` opens the log of the Logbook last used on the device; sign-in is `/login` and registration `/register` (see Sign-in and Credentials). The document title is the name of the screen alone, as in `Live entry` or `QSL Management`; a Station, an Instrument and a settings section add the screen after their own name (`Home - Stations`, `IC-705 - Instruments`, `Account - Settings`). A screen without a name of its own is `JL1HIS Logger`.
- The log keeps its query in the URL as `?q=`, in the text syntax of Search, so a query can be bookmarked and reopened later. The QSO open in the detail overlay is `?qso={qso_id}`. Typing a query, opening or closing the overlay replace the current history entry rather than adding one. Checked rows stay out of the URL.
- The columns of the log table on wide screens are a per-device setting shared by every Logbook: the ordered list of shown columns, left to right, kept in local storage. The built-in columns are Date / UTC (one column), Call, QSL S / R, Band, Freq, Mode, RST S / R, Station and Park, shown by default, and LoTW S / R, eQSL S / R, TX Power, Contest (`CONTEST_ID`) and Number S / R (`STX_STRING` / `SRX_STRING`), hidden by default; any ADIF QSO field or `APP_` field can be added as a column of its own, named `adif.{FIELD}` as in Search. The check box column stays first. The narrow-screen list keeps its fixed layout.
- On wide screens the sidebar can be collapsed to a narrow bar of icons, a per-device setting kept in local storage. Collapsed, it shows the Logbook switcher as an icon, the sync state as a dot whose text is its tooltip and the user's callsign turned on its side, reading top to bottom; it never collapses on its own. Mobile keeps its tab bar.

### Localization

- The web client's text is localized. The locales are English, the source every message is written in first, and Japanese. Messages are Paraglide JS (inlang) message files, one per area of the client and locale (`client/messages/en/log.json`, `client/messages/ja/log.json` and so on, listed in `client/project.inlang/settings.json`), compiled into typed functions in `client/src/lib/paraglide/`, so a missing or misspelled key fails `svelte-check`. Message keys start with their area (`log_`, `entry_`) except the shared ones in `common.json`. A test keeps the Japanese keys equal to the English ones; at run time a missing message would fall back to English.
- The inlang plugin that reads the message files is the `@inlang/plugin-message-format` package, loaded from `node_modules` by path, so compiling the messages never fetches code from the network.
- The locale is a per-device setting kept in local storage, chosen at `/settings/language`. Without a stored choice it is the first of the browser's languages that is a locale, or else English. The locale is never part of the URL, so every path stays as in Web Client Routes. The document's `lang` follows it, and changing it reloads the page.
- The display language and the speech language of voice entry are separate settings; changing one never changes the other.
- Translated: labels, buttons, headings, notices, warnings, errors, document titles (the English titles in Web Client Routes are the source) and the display names of fields and search chips (`My park`, `QSL sent`). Numbers are formatted with `Intl` for the locale, and plurals belong to the messages.
- Not translated: ADIF field names and values, enumerations (bands, modes, QSL values such as `R` or `Y`), callsigns, the keywords of the query syntax (`call:`, `no:`) and the words of the voice syntax, which follow the speech language. QSO dates and times stay ISO dates and UTC `hh:mm` in every locale. The server, the CLI, logs, `/wl2` responses and ADIF output stay English.
- The server keeps the English `error` text of its errors, for API clients and logs. An error the web client words for the user carries a stable snake_case `code` and its parameters as fields beside it, as a query error already does (`code`, `term`, `index`, `start`, `end`), and the query crate's `ErrorKind` codes serve the server's `400` and the client's WASM chips alike. The client words an error from its code, and shows the `error` text as it is for a code it does not know. Codes are part of the API: renaming one is a breaking change.
- Codes are given to errors a user can reach through the web client: a QSO that does not check (`qso_missing`, `qso_invalid`, `field_repeated` and the other `qso_*` codes), a Station's, Instrument's or area code field's fields, names, Memberships and tokens, passkeys and registration links, import and export batches, conflicting updates, query terms and things not found (`not_found` with the thing in `what`). In the store, `StoreError::Invalid` and `StoreError::Conflict` carry a `Fault` whose code and parameters the web layer adds to the body; a fault without a code keeps only its text. Errors only an API client can cause, such as a malformed `If-Match` or `Last-Event-ID`, and errors of instance administration have no code. Warnings are already a `kind` with parameters and are worded by the client.

### Offline Entry

- The client keeps no copy of the Logbook, so offline there is no log list, no lookup and no duplicate warning.
- Entry keeps only its own QSOs. Live entry and voice entry write each QSO to their shared queue in IndexedDB and send it at once. A QSO that could not be sent stays in the queue with the reason. The screen shows the queued QSOs of the chosen Station, and the user sends those explicitly or discards one; nothing retries in the background. Resending is safe because `POST` is idempotent on the UUID, and duplicate warnings come back in the `warnings` of the send.
- The service worker caches the app shell, so the PWA starts offline; it never caches `/api` or `/wl2`.

### Live and Voice Entry

- Voice entry is one screen of the PWA and shares the offline queue with Live entry.
- Voice entry shares Live entry's per-device settings (Station, Instrument, frequency, band, mode and contest, the serial included), so switching between the two screens carries them over. The spoken QSL requests map as follows: "requested" sets `QSL_SENT = R`, "one way" sets `QSL_SENT = N` and `QSL_RCVD = R`, and the area code is spoken as "area" (Japanese "エリア") followed by spelled letters and digits, up to 16 characters, which go to the Logbook's area code field, or nowhere, with a note on the screen, when the Logbook has none. There are no JCC / JCG words in the voice syntax.
- Live entry takes the Logbook's area code field, when it has one, and the same QSL request as voice entry (none, requested, one way) on one row under the name and QTH. It has a collapsed extra fields panel: `TX_PWR` (watts, under the Instrument's rule), `NOTES`, and any other ADIF 3.1.8 QSO field that is not import-only, or an `APP_` field, typed by name. A field the form already has, `STATION_CALLSIGN`, an unknown name or a name given twice blocks logging. The other fields of a Station or an Instrument are checked against the same list of names. A value given there overrides the baked values of the Instrument and the Station, such as the Instrument's `TX_PWR`. TX power stays for the next QSO while the screen is open, and is cleared when the Instrument changes; while it is empty, the field shows the Instrument's `TX_PWR` as its placeholder. Notes and the other fields are cleared with the QSO.
- The QSO editing screen edits the fields the entry screens write: call, date and time, frequency, band, mode and submode, the reports, name, QTH, the Logbook's area code field, comment, notes, `TX_PWR`, and the paper QSL state (`QSL_SENT`, `QSL_RCVD`, both `_VIA` and both dates). Every other field of the QSO is listed by name under "Other ADIF fields", where it can be changed, removed, or added with the same rules as the extra fields of Live entry, except that `STATION_CALLSIGN`, `OPERATOR` and the `MY_*` fields are shown read-only as the Station at the time. LoTW and eQSL states are shown, not edited. The band, frequency, sideband and default reports follow each other as in Live entry: a changed band or frequency moves `LSB` / `USB` with it, and a changed mode replaces a report still at the previous mode's default.
- Live entry and voice entry choose the Instrument with a select next to the Station's, and the QSO editing screen chooses it as well. None is a valid choice. The voice syntax does not change it. A remembered Instrument that is archived or gone is cleared. The queue holds the Instrument's ID, and the server bakes its values in when the QSO is sent, as it does for the Station. When the user has favorites, the Station and Instrument selects show them first in a group of their own, and a Station not yet chosen on the device defaults to the first favorite. Every row of the Station and Instrument lists has a star at its left that marks and unmarks the favorite.
- Speech recognition is Vosk in the browser. The models are not in the repository: `client/scripts/setup-vosk.sh` (also `pnpm run models`) or `client/scripts/setup-vosk.ps1` downloads them into `client/static/models`, and puts each model's vocabulary into `client/.vosk` for the vocabulary tests, which read it when `VOSK_WORDS_EN` or `VOSK_WORDS_JA` is not set. The server serves them with the app, and the browser keeps a loaded model in IndexedDB so voice entry works offline. The service worker does not precache them. The Vosk worker is copied out of `vosk-browser` into `static/vosk-worker.js` at build time and started as a same-origin worker, so the page's CSP needs neither `blob:` workers nor `unsafe-eval`.

### Reference Data

- Updates are done with CLI subcommands, which check a local file and switch to its version. The web UI only shows the versions.
- `reference install <kind> <file>` installs a file the admin has downloaded or written: it parses and checks the file, puts it where the configuration points (through a staging file and a rename), and records the version with the file's SHA-256 in `reference_versions`. Validation and database update errors preserve the previously installed file and version; if committing fails after replacement, the CLI restores the previous file (or removes the new file on a first installation). The server fetches nothing over the network. The version of `cty.dat` comes from its `=VER` entry unless one is given; the others default to the UTC date of the installation.
- The kinds are `cty` (AD1C `cty.dat`), `dxcc`, `subdivisions` and `parks`. `cty.dat` carries no ADIF DXCC entity codes, so `dxcc` is a TOML table from the primary prefixes `cty.dat` declares (`JA`, `JD/o`) to the codes. It was generated once and committed as `assets/dxcc.toml` for `reference install dxcc`; the generator is not kept. Each `cty.dat` entity took the code whose current prefix or name matched in a table of DXCC entities, checked against the names of the ADIF 3.1.8 DXCC Entity Code enumeration, and the two it did not match (`PY0S`, `PY0T`) were set by hand. All 340 current entities have a code, each used once, none deleted. `subdivisions` (`[339]` tables of codes and names) and `parks` (`JP-0001 = { name_en, name_ja }`) are TOML as well.
- The server reads a data set when it is first needed and keeps it until the version or checksum recorded for its kind changes, so an installation by the CLI takes effect without a restart. A file that no longer matches its recorded checksum is an error rather than being served. A kind that was never installed is simply absent: lookup answers without the entity, and the subdivision catalog is `404`.
- DXCC and zones already recorded on existing QSOs are not changed automatically by a reference data update.
- The native API serves the subdivisions of one DXCC entity from the installed `subdivisions` data, so that the client offers `STATE` and `MY_STATE` as a choice (prefectures for 339): `GET /api/reference/subdivisions/{dxcc}` answers `[{code, name}]` ordered by code, to any signed-in user as the versions are. An entity without subdivisions is an empty list and the kind not installed is `404`; in both cases the client falls back to free text. The lookup is shared with the `subdivisions` topic of the `/wl2` catalog.
- The client asks for a list when a form needs it and keeps it no longer than the `subdivisions` version it saw in `GET /api/reference`. No other data set is served: parks for display and `cty.dat` for offline lookup are not.
- Modes and submodes are the ADIF Mode and Submode enumerations in full, embedded in the server like the band table, with the import-only modes marked. The table follows ADIF 3.1.8; where its Mode and Submode tables disagree, it takes the union. An import-only mode stands for the mode it is a submode of. A QSO whose `MODE` is unknown, whose `SUBMODE` is unknown, or whose `SUBMODE` belongs to another mode is not refused; creation reports it in `warnings` and an import preview reports it per record. The client keeps the same table, checked against the server's through `fixtures/reference/modes.json`. The names spoken in voice entry (`USB`, `FT4` and so on) belong to the client.
- JCX is handled as a string, keeping leading zeros.

### Migrations and Deployment

- The server reports its package version and the Git commit it was built from (`GET /api/version`), shown in the client's server settings. `build.rs` takes the commit from `GIT_COMMIT` when it is set and from `git rev-parse HEAD` otherwise; the Docker build has no `.git`, so the Image workflow passes the commit as the `GIT_COMMIT` build argument. A build that knows neither reports no commit.
- Migrations are embedded in the binary with `sqlx::migrate!()` and applied forward-only. `serve` applies the pending ones at startup; a `migrate` subcommand applies them explicitly, and a configuration setting turns the automatic run off for a deployment that wants the explicit step.
- sqlx records each applied migration with its checksum in `_sqlx_migrations`, so an edited applied file stops the server at startup instead of running.
- Before applying pending migrations, the server snapshots the database with `VACUUM INTO` under the data directory. There is no downgrade; rolling back means restoring that snapshot.
- One process owns the database file, so there is no separate migration job or init container. A deployment is one container holding the binary and `client/build`, with the data directory (configuration, database, reference data, snapshots) on a volume and TLS terminated outside.
- Every setting is in one TOML configuration, `config.toml` in the working directory unless `-c` names another; nothing is read from the environment but `RUST_LOG`, and there is no `.env`. Relative paths in it are relative to its own directory, so the data directory holds the configuration with what it points to. `serve --listen` and `--web-dir` override the file, so that a container image carries its own listen address and web app; the CLI names its user with `--as` only.
- The server accepts connections itself with hyper's automatic HTTP/1 and HTTP/2 handling: request headers must arrive within 30 seconds, a request body may pause for at most 30 seconds between chunks, and HTTP/2 connections are pinged every 30 seconds and dropped after 10 seconds without an answer. Shutdown stops accepting, ends the event streams and gives open connections 10 seconds. The reverse proxy adds its own limits, HSTS and request rate limits.
- Query macros that need the schema at compile time keep their `cargo sqlx prepare` output in the repository, refreshed whenever a migration is added.

### Lookup

- The native lookup, `GET /api/logbooks/{id}/lookup?call=`, returns the `cty.dat` entity (name, ADIF DXCC code when `dxcc` is installed, continent, CQ and ITU zones, position) and the previous contacts in that Logbook matched by `call_base`: the latest non-empty `NAME`, `GRIDSQUARE`, `QTH`, `STATE`, `CNTY`, `IOTA` and `QSL_VIA` under their ADIF names, the number of QSOs and the time of the last one. It can be narrowed to Stations.
- The work lives in `domain` and `store`; `/wl2` lookup only reshapes the result into Wavelog's response.
- The client asks the server. It keeps no copy of the Logbook, so it has no previous contacts to show while offline (see Offline Entry).

## Remaining Work

- The CLI takes the text query of Search: `export write` still takes only the structured filters.
- The exported ADIF for QSL cards, contest logs and POTA is checked against real submissions; then the writer moves into adif-reader (see ADIF Input and Output).
