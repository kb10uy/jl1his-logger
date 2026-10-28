# Native API

The API the web client uses, under `/api` on `public_origin`. Bodies are
JSON. It covers sign-in, the server version and health, reference versions,
API tokens, favorite frequencies, Logbooks, Members, Stations, Instruments, QSOs with their event
stream, bulk QSL changes, export batches and ADIF import. Responses are
`Cache-Control: no-store` and carry the security headers listed in
[decisions.md](../development/decisions.md).

## Callers

A request is authenticated by the session cookie the web client holds, or by a
an API token sent as `Authorization: Bearer <token>` (see API Tokens
below). When `Authorization` is present the cookie is ignored. Not every
endpoint takes a token:

| Endpoints | Session cookie | API token |
| --- | --- | --- |
| `GET /api/health` | Not needed | Not needed |
| `POST /api/auth/login/*`, `POST /api/auth/register/*` | Issued by them | No |
| `GET /api/me`, `/api/frequencies` | Yes | Yes |
| `GET /api/logbooks` and everything under `/api/logbooks/{logbook_id}`: Logbooks, Members, Stations, Instruments, QSOs, Lookup, Bulk QSL Changes, Export Batches, ADIF Import, the Event Stream | Yes | Yes, for the Logbooks of its scopes |
| `POST /api/auth/logout`, `/api/passkeys`, `/api/sessions`, `/api/tokens`, `GET /api/version`, `/api/reference` | Yes | No: `401` |

A token reaches only the Logbooks of its scopes where its user still has a
Membership. Under
`/api/logbooks/{logbook_id}` the caller's Membership is checked before the
request itself, so a caller outside the Logbook gets `404` whatever it sends.
A Logbook, Station, Instrument or QSO the caller cannot reach is `404`, the
same as one that does not exist. A write reserved for owners is `403` for an
operator. State-changing requests must come from `public_origin`.

## Errors

An error is `{"status": "error", "error": "<message>"}` with the HTTP status:

| Status | When |
| --- | --- |
| `400` | An invalid request, or an expired passkey request |
| `401` | Not signed in, or a passkey or registration link is refused |
| `403` | A cross-origin write, or a missing right |
| `404` | Anything not found or not visible |
| `409` | A conflict |
| `412` | `If-Match` names another revision |
| `415` / `422` | A body that is not the expected JSON |
| `428` | `If-Match` is missing |

Some errors add fields next to `error`, such as `current` on a revision
conflict. An error a user can meet in the web client also has a stable `code`
and its parameters beside it, so the client can word it in the user's
language; `error` stays English.

| `code` | Parameters | When |
| --- | --- | --- |
| `not_found` | `what`: `QSO`, `Station`, `Instrument`, `Logbook`, `user`, `passkey`, `API token`, `favorite frequency`, `export batch` or `Membership` | The thing does not exist or is not visible |
| `qso_missing` | `field` | A required QSO field is absent |
| `qso_invalid` | `field`, `value` | A QSO field holds a value that is not valid |
| `qso_field_name` | `field` | Not an ADIF field name |
| `qso_column_field` | `field` | A column field was given among the extra fields |
| `qso_too_long` | `field`, `bytes`, `max` | A value is longer than a field may be |
| `qso_too_many_fields` | `count`, `max` | The QSO has more extra fields than are kept |
| `field_repeated` | `field` | A field is given more than once |
| `field_not_allowed` | `field` | A field that cannot be a Station or Instrument default |
| `instrument_field` | `field` | `MY_RIG`, `MY_ANTENNA` or `TX_PWR` given to a Station |
| `area_field_not_allowed` | `field` | A field that cannot be the Logbook's area field |
| `name_required` | | A Station, Instrument or token without a name |
| `name_too_long` | `bytes`, `max` | A name longer than is kept |
| `last_owner` | | Removing or demoting the Logbook's last owner |
| `last_passkey` | | Deleting the user's last passkey |
| `passkey_registered` | | The passkey is already registered |
| `passkey_failed` | | The authenticator's answer was refused |
| `passkey_expired` | | The ceremony has expired |
| `passkey_unknown` | | The passkey is not registered |
| `registration_link_invalid` | | The registration link is unknown or used up |
| `not_a_callsign` | `callsign` | The member's callsign is not a callsign |
| `token_needs_logbook` | | A token without a Logbook |
| `favorite_frequency_invalid` | | A favorite frequency without a frequency in MHz or a known mode |
| `qso_deleted` | `id` | Updating a deleted QSO |
| `qso_changed` | `id` | The QSO changed while it was being updated |
| `id_taken` | `id` | The ID belongs to another QSO |
| `revision_conflict` | `id` | `If-Match` names another revision |
| `rewrite_count_changed` | `count`, `expected` | The QSOs a rewrite would change no longer match the previewed count |
| `too_many_values` | `count`, `max` | A filter lists more values than are taken |
| `batch_empty` | | An export selects no QSOs |
| `batch_applied` | `id` | The export batch has already been applied |
| `batch_nothing_to_apply` | `kind` | Batches of this kind have nothing to apply |
| `not_utf8` | | An imported file is not UTF-8 |
| `not_encoding` | `encoding` | An imported file is not in the named encoding |
| `not_adif` | `detail` | An imported file is not ADIF |
| `unencodable` | `record`, `field`, `character` | An export holds a character the encoding cannot write |
| `reference_invalid` | | A CTY archive was refused |

The query codes of `q` are listed under Search.

## Sign-in

WebAuthn options are the webauthn-rs JSON (`publicKey` with base64url binary
fields). A start returns a `ceremony` ID that its finish sends back with the
browser's `credential`; a ceremony is used once and expires after 5 minutes.
Of these endpoints, only `GET /api/me` takes a `native` API token; the rest
work with the session cookie alone.

| Request | Body | Response |
| --- | --- | --- |
| `GET /api/me` | | The signed-in user `{id, callsign, role}`, renewing the cookie; with a `native` API token, the token's user |
| `POST /api/auth/login/start` | | `{ceremony, options}` for a discoverable credential |
| `POST /api/auth/login/finish` | `{ceremony, credential}` | The user, with the session cookie |
| `POST /api/auth/logout` | | `204`, deleting the session and the cookie |
| `POST /api/auth/register/start` | `{token}` from the registration link | `{ceremony, options, callsign}` |
| `POST /api/auth/register/finish` | `{ceremony, credential, name?}` | The user, with the session cookie; the link is used up |
| `GET /api/passkeys` | | The signed-in user's passkeys `[{id, name, created_at, last_used_at}]` |
| `POST /api/passkeys/register/start` | | `{ceremony, options}`, excluding the user's passkeys |
| `POST /api/passkeys/register/finish` | `{ceremony, credential, name?}` | `201` with the new passkey |
| `DELETE /api/passkeys/{id}` | | `204`; `409` for the user's last passkey |

`role` is `admin` or `user`. A passkey `name` left out or blank becomes
`Passkey`.

## Sessions

A session is `{id, user_id, user_agent, created_at, last_used_at, expires_at,
current}`: `user_agent` is the browser's `User-Agent` at sign-in, possibly
`null`; `last_used_at` and `expires_at` move at most once a day; `current` marks
the session making the request. A session ends 30 days after its last use and
90 days after it was created at the latest. These endpoints take the session
cookie only.

| Request | Body | Response |
| --- | --- | --- |
| `GET /api/sessions?user_id=` | | The live sessions of the signed-in user, or of `user_id` for an admin |
| `DELETE /api/sessions/{id}` | | `204`; the session's user or an admin |

## Server Version and Health

`GET /api/version` returns `{version, commit, adif}`: the server's package
version, the Git commit it was built from, or `null` when the build did not
know it, and the ADIF version its built-in tables follow (`3.1.8`), which is
also the `ADIF_VER` of its exports.
It takes the session cookie only.

`GET /api/health` needs no sign-in and answers `{"status": "ok"}` when the
database can be read, or `503 {"status": "error"}`, for a reverse proxy or a
container runtime to probe.

## Reference Data

`GET /api/reference` returns the versions of the reference data in use,
`[{kind, version, updated_at}]` with `kind` `cty`, the one kind. It takes the session cookie only. Reference data is updated with the
CLI or with the endpoint below.

`PUT /api/reference/cty` installs a CTY zip archive from country-files.com (Big
CTY or the contest files), sent as the raw request body, and answers the
installed `{kind, version, updated_at}`. It takes the session cookie of an
admin only (`403` for other users, `401` for API tokens) and, like every write,
must come from `public_origin`. The body is limited to 4 MiB (`413` beyond it).
The archive must hold `cty.dat` and `cty.csv` at its root whose primary
prefixes match one to one; the version is the `=VER` entry of `cty.dat`, or
the UTC date of the installation without one. A refused archive is `400` with
`code` `reference_invalid` and the reason in `error`, and leaves the installed
data as it was. See "Reference Data" in [decisions.md](../development/decisions.md) for the limits applied
while reading it.

`GET /api/reference/subdivisions/{dxcc}` returns the subdivisions of one DXCC
entity from the ADIF Primary Administrative Subdivision table built into the
server, `[{code, name}]` ordered by code, for the `STATE` and `MY_STATE`
choices; subdivisions the specification marks deleted or import-only are left
out. It also takes the session cookie only. An entity without subdivisions is
`[]`, a `dxcc` that is not a number is `400`, and a number that is not an ADIF
DXCC entity code is `404`.

## API Tokens

A token is `{id, user_id, name, logbook_ids, created_at, last_used_at}`.
`last_used_at` is updated at most once a day. These endpoints take the session cookie only, so a token cannot
manage tokens.

| Request | Body | Response |
| --- | --- | --- |
| `GET /api/tokens?user_id=` | | The tokens of the signed-in user, or of `user_id` for an admin |
| `POST /api/tokens` | `{name, logbook_ids}` | `201 {token, plaintext}` |
| `DELETE /api/tokens/{id}` | | `204`; the token's user or an admin |

The plaintext is shown only in the response that issues it. Every Logbook in
`logbook_ids` needs a Membership of the user, or it is `404`. Another user's
token is `404` for anyone but an admin.

## Favorite Frequencies

A favorite frequency is `{id, freq, mode, submode, created_at}`: a frequency
in MHz with a mode, kept for the caller's user alone and shared by every
Logbook. `freq` is formatted by the server (`7.1`, `145`), and `submode` is
`""` when there is none. These endpoints take the session cookie or an API
token, which reaches the favorites of its user whatever its scopes.

| Request | Body | Response |
| --- | --- | --- |
| `GET /api/frequencies` | | The user's favorites by frequency, then mode and submode |
| `POST /api/frequencies` | `{freq, mode, submode}` | The favorite; the one already kept for the same frequency, mode and submode |
| `DELETE /api/frequencies/{id}` | | `204` |

`freq` is a number of MHz above zero, `mode` an ADIF mode that is not
import-only, and `submode`, which may be left out, empty or one of its
submodes; anything else is `400 favorite_frequency_invalid`. Another user's
favorite is `404`.

## Logbooks

A Logbook is `{id, name, role, cursor_generation, last_seq, last_updated_seq,
area_field}`, where `role` is the caller's Membership, `owner` or `operator`,
and `area_field` is the Logbook's area code field, `{name, label}` such as
`{"name": "CNTY", "label": "JCC / JCG"}`, or `null`. These endpoints, and
every one under `/api/logbooks/{logbook_id}` in the sections that follow, take
the session cookie or a `native` API token.

| Request | Body | Response |
| --- | --- | --- |
| `GET /api/logbooks` | | The caller's Logbooks, by name |
| `GET /api/logbooks/{logbook_id}` | | One Logbook |
| `PUT /api/logbooks/{logbook_id}/settings` | `{area_field}`, `{name, label}` or `null` | The Logbook; owners only |

The area code field's name is an ADIF field name, read ignoring case and
stored upper-cased, and its label is required. `CALL`, `QSO_DATE`, `TIME_ON`,
`BAND`, `MODE`, `STATION_CALLSIGN`, `OPERATOR` or a `MY_*` field is `400` with
`area_field_not_allowed`. Changing
the field moves no recorded values.

## Members

A member is `{user_id, callsign, role, disabled}`. Members are named by
callsign, since an owner cannot list the users of the instance; owners cannot
create users either. Paths are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `GET /members` | | The members, by callsign |
| `PUT /members/{callsign}` | `{role}`, `owner` or `operator` | The members after adding the user or changing their role; owners only |
| `DELETE /members/{callsign}` | | `204`; owners only |

An unknown or disabled user is `404`. Demoting or removing the last active
owner is `409`.

## Stations

A Station is `{id, name, callsign, fields, archived_at, created_at,
updated_at, favorite}`. `fields` holds the `MY_*` and other defaults baked
into new QSOs. Paths are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `GET /stations` | | The Stations: your favorites first, then the others by name, archived ones last |
| `GET /stations/{id}` | | One Station |
| `POST /stations` | `{name, callsign, fields}` | `201` with the Station; owners only |
| `PUT /stations/{id}` | `{name, callsign, fields}` | The Station; owners only |
| `POST /stations/{id}/archive` | | The archived Station; owners only |
| `DELETE /stations/{id}/archive` | | The Station, no longer archived; owners only |
| `POST /stations/{id}/favorite` | | The Station, now one of your favorites |
| `DELETE /stations/{id}/favorite` | | The Station, no longer one of your favorites |
| `GET /stations/{id}/rewrite?from=&until=` | | `{count}` of past QSOs a rewrite would change; owners only |
| `POST /stations/{id}/rewrite` | `{from?, until?, count}` | `{count}` rewritten; owners only |

The callsign is upper-cased and field names are read ignoring case; an empty
value leaves the field out. `CALL`, `QSO_DATE`, `TIME_ON`, `BAND`, `MODE` and
`STATION_CALLSIGN` cannot be defaults, and `MY_RIG`, `MY_ANTENNA` and `TX_PWR`
belong to Instruments, so a Station with any of them is `400`.

Editing a Station never changes past QSOs. A rewrite does: it covers the
Station's live QSOs with a QSO time in `[from, until)` (RFC 3339 times, both
optional) and sets `STATION_CALLSIGN` and every field of the Station, leaving
other fields alone. Its `count` must equal what a preview returns now, or it is
`409` and nothing changes.

`favorite` is whether the Station is a favorite of the caller: the signed-in
user, or the user an API token belongs to. Any member marks and unmarks
favorites, including operators and archived Stations; doing it twice changes
nothing and leaves `updated_at` alone.

## Instruments

An Instrument is `{id, name, my_rig, my_antenna, tx_pwr, fields, archived_at,
created_at, updated_at, favorite}`: the equipment baked into new QSOs as `MY_RIG`,
`MY_ANTENNA` and `TX_PWR`, each `null` when unset, with `fields` holding other
defaults that win over the Station's. Any member manages them; only owners
rewrite their past QSOs. Paths are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `GET /instruments` | | The Instruments: your favorites first, then the others by name, archived ones last |
| `GET /instruments/{id}` | | One Instrument |
| `POST /instruments` | `{name, my_rig?, my_antenna?, tx_pwr?, fields}` | `201` with the Instrument |
| `PUT /instruments/{id}` | `{name, my_rig?, my_antenna?, tx_pwr?, fields}` | The Instrument |
| `POST /instruments/{id}/archive` | | The archived Instrument |
| `DELETE /instruments/{id}/archive` | | The Instrument, no longer archived |
| `POST /instruments/{id}/favorite` | | The Instrument, now one of your favorites |
| `DELETE /instruments/{id}/favorite` | | The Instrument, no longer one of your favorites |
| `GET /instruments/{id}/rewrite?from=&until=` | | `{count}` of past QSOs a rewrite would change; owners only |
| `POST /instruments/{id}/rewrite` | `{from?, until?, count}` | `{count}` rewritten; owners only |

`my_rig`, `my_antenna` and `tx_pwr` are trimmed, and an empty one is `null`.
`tx_pwr` is a decimal number of watts above zero with at most six decimal
places, such as `0.5` or `100`; anything else is `400`. Field names are read
ignoring case and an empty value leaves the field out, as for Stations;
`MY_RIG`, `MY_ANTENNA`, `TX_PWR`, `CALL`, `QSO_DATE`, `TIME_ON`, `BAND`,
`MODE` and `STATION_CALLSIGN` cannot be in `fields`.

Editing an Instrument never changes past QSOs. A rewrite works as a Station's
does over the QSOs whose `instrument_id` is the Instrument: it sets `MY_RIG`,
`MY_ANTENNA` and `TX_PWR` where the Instrument has them, and every field of
its `fields`, leaving the others alone.

`favorite` works as for Stations.

## QSOs

A QSO is:

```json
{
  "id": "019a0a4e-6f2b-7c3d-8e4f-0123456789ab",
  "station_id": "...",
  "instrument_id": null,
  "seq": 1,
  "updated_seq": 1,
  "revision": 1,
  "created_by": "<user ID>",
  "created_by_callsign": "JA1OPR",
  "created_at": "2026-10-22T04:05:06Z",
  "updated_by": "<user ID>",
  "updated_by_callsign": "JA1OPR",
  "updated_at": "2026-10-22T04:05:06Z",
  "deleted_at": null,
  "fields": { "CALL": "JA1XYZ", "QSO_DATE": "20261022", "TIME_ON": "040506", "...": "..." }
}
```

`fields` holds every ADIF field of the QSO, as the strings they arrived as;
which of them are columns is not visible. Names in a request are read ignoring
case and stored upper-cased; an empty value is the same as an absent field.
A name is ASCII letters, digits and underscores starting with a letter, a
value at most 2048 bytes, and a QSO has at most 1024 fields outside the
columns; anything else is `400`. A comma-separated filter takes at most 100
values. `CALL`, `QSO_DATE`, `TIME_ON`, `BAND` and `MODE` are required. A deleted QSO
keeps its fields and has `deleted_at` set. `created_by_callsign` and
`updated_by_callsign` are those users' current callsigns.

Paths below are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `POST /qsos` | `{id, station_id, instrument_id?, fields}` | `201 {qso, warnings}`; `200 {qso, warnings: []}` with the existing row when the ID is known |
| `GET /qsos/{id}` | | The QSO, deleted or not |
| `PUT /qsos/{id}` with `If-Match` | `{station_id, instrument_id?, fields}` | `{qso, warnings}` |
| `DELETE /qsos/{id}` with `If-Match` | | The deleted QSO; deleting a deleted QSO returns it unchanged |
| `GET /qsos?...` | | `{qsos, next, total}`, see Search |
| `GET /targets?...` | | `{qsos: [{id, revision}]}` of every match, see Search |
| `GET /changes?after=N&limit=L` | | `{cursor_generation, last_updated_seq, qsos, has_more}` |
| `GET /duplicates?call=&band=&mode=&qso_date=&time_on=&except=` | | `{qsos}` |

`id` is a UUID v7 the client generates; anything else is `400`.
`instrument_id` is optional and `null` means none; an Instrument or Station
outside the Logbook is `404`. Creating fills the Instrument's `MY_RIG`,
`MY_ANTENNA`, `TX_PWR` and other defaults, then `STATION_CALLSIGN` and the
Station's `MY_*` defaults, then `OPERATOR` from the caller's callsign, for
fields the request leaves out. It also fills a
missing `CQZ`, `ITUZ` and `CONT` from the entity `cty.dat` resolves `CALL` to,
and `DXCC` from `cty.csv`; without `cty` installed, or when
it cannot be read (which is logged), nothing is filled and the QSO is still
created. `PUT` never fills them. An `id` that belongs
to another Logbook is `409`.

Every response carrying one QSO has an `ETag` of its revision in quotes, such
as `"3"`. `PUT` and `DELETE` require `If-Match` with the revision they are based
on, in the same form; without it they are `428`, and a weak tag, `*` or several
tags are `400`. Another revision is `412`, and a `PUT` on a deleted QSO is
`409`; both carry the current QSO as `current`, so the client merges and retries
with its revision.

`PUT` replaces the whole QSO: `fields` is the complete new set, Station and
Instrument defaults are not baked in again, and a field left out is removed.
It sets the Instrument to `instrument_id`, so leaving it out clears it.

`warnings` never refuse a write:

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `possible_duplicate` | `ids` | Live QSOs with the same call, band and mode, ignoring case, within 15 minutes |
| `frequency_outside_band` | `freq`, `band` | `FREQ` is outside `BAND` |
| `unknown_mode` | `mode` | `MODE` is neither an ADIF mode nor an import-only one |
| `unknown_submode` | `submode` | `SUBMODE` is not an ADIF submode |
| `submode_of_another_mode` | `mode`, `submode`, `expected` | `SUBMODE` belongs to the mode `expected`, not to `MODE` |

Modes are the Mode enumeration of ADIF 3.1.8, compared ignoring case; the table
is generated from `assets/adif/enumerations_mode.csv` and
`enumerations_submode.csv`. An import-only mode such as `C4FM` or
`PSK31` is accepted and stands for the mode it is a submode of
(`DIGITALVOICE`, `PSK`), so `MODE` `DSTAR` with no `SUBMODE` warns of nothing.

`/duplicates` runs the same check for a QSO being entered and returns the
matching QSOs by QSO time; `except` leaves out the QSO being edited and must be
a QSO ID.

### Changes

`/changes` returns the QSOs with `updated_seq > after` (default 0), deleted ones
included, by `updated_seq`, at most `limit` (default and maximum 1000). While
`has_more` is true, the next request continues from the `updated_seq` of the
last row. The response is read from one snapshot: `last_updated_seq` is the
Logbook's latest `updated_seq` in it, and no row goes beyond it. A client whose stored `cursor_generation` differs discards its
cursor and starts from 0.

### Search

`GET /qsos` returns live QSOs in the order of `sort`, at most `limit`
(default 100, maximum 1000). `sort` is one of:

| `sort` | Order |
| --- | --- |
| `-time` (default) | Newest first by QSO time |
| `time` | Oldest first by QSO time |
| `call` | By the call without prefixes and suffixes, A to Z, newest first within a call |
| `-call` | The same, Z to A, still newest first within a call |

Each filter is optional:

| Parameter | Matches |
| --- | --- |
| `from`, `until` | QSO time in `[from, until)`, as RFC 3339 times |
| `station_id` | The Station |
| `band`, `mode` | Ignoring case |
| `call` | The call exactly, ignoring case |
| `call_base` | The call without prefixes and suffixes (`JA1XYZ` matches `JA1XYZ/P`); not together with `call` |
| `qsl_sent` | Comma-separated `QSL_SENT` values ignoring case; an empty value matches an absent field |
| `contest_id` | `CONTEST_ID`, ignoring case |
| `q` | The text query of [query.md](query.md) (`call:JA1* -mode:FT8 date:2026-10-01..`) |

All the filters given must hold. `next` is an opaque cursor for the following
page, passed back as `cursor` with the same `sort`, or `null` on the last page.
A cursor of another sort order is `400`. The first page, the
one without a `cursor`, also carries `total`, the number of matches.

A `q` that cannot be read is `400` and names the term:

```json
{
  "status": "error",
  "error": "freq is not a search field (term \"freq:7\" at 9)",
  "code": "unknown_field",
  "term": "freq:7",
  "index": 1,
  "start": 9,
  "end": 15,
  "value": "freq"
}
```

`index` counts the terms from 0; `start` and `end` are positions in the query
in Unicode scalar values. `code` is one of `unknown_field`, `bad_field_name`,
`missing_value`, `too_many_values`, `bad_date`, `bad_number`, `unclosed_quote` and
`empty`. `value` holds the field name, date or number at fault for
`unknown_field`, `bad_field_name`, `bad_date` and `bad_number`, and
`too_many_values` adds `count` and `max`.

`GET /targets` takes the same filters, without `sort`, `cursor` and `limit`, and returns
the ID and revision of every match, newest first. Checking every match in the
log list uses it, and a bulk QSL change takes the result as it is.

## Lookup

`GET /api/logbooks/{logbook_id}/lookup?call=&station_ids=` answers what the
reference data and the Logbook know about a callsign:

```json
{
  "call": "JA1XYZ",
  "entity": {
    "name": "Japan", "dxcc": 339, "continent": "AS", "cq_zone": 25, "itu_zone": 45,
    "latitude": 36.4, "longitude": 138.38
  },
  "previous": {
    "count": 2,
    "last_qso_at": "2026-10-22T03:00:00Z",
    "fields": { "NAME": "Hanako", "QTH": "Chiba", "GRIDSQUARE": "PM95" }
  }
}
```

`call` is required and answered upper-cased. `entity` is what `cty.dat`
resolves the call to, with the overrides of the matching alias and longitude
growing towards the east, and `dxcc` is the entity code from `cty.csv`; it is
`null` when `cty` is not installed or `cty.dat` does not know the call. `previous` covers the live QSOs whose call has the same base
(`JA1XYZ` matches `JA1XYZ/P`), optionally narrowed by `station_ids`, a
comma-separated list of Station IDs: their number, the QSO time of the latest,
and the newest nonempty `NAME`, `GRIDSQUARE`, `QTH`, `STATE`, `CNTY`, `IOTA`
and `QSL_VIA` across all of them.

## Bulk QSL Changes

`POST /api/logbooks/{logbook_id}/qsl` takes
`{qsos: [{id, revision}], fields: {...}}` and sets the given QSL fields on each
QSO at its revision. Only the QSL fields (`QSL_SENT`, `QSL_RCVD`, `*_VIA`,
`QSLSDATE`, `QSLRDATE`, `EQSL_QSL_*`, `LOTW_QSL_*`) are accepted, with values
checked against the ADIF enumerations ignoring case and dates as `YYYYMMDD`, and
stored as given. Any other field or value is `400` and changes nothing.

The response is `{updated, skipped}`: `updated` holds the changed QSOs, and
`skipped` the others as `{id, reason}` with `reason` `missing`, `deleted` or
`revision` (with `current_revision`).

## Export Batches

An export selects QSOs, writes them as ADIF and records them with their
revisions as a batch, so that sending them can later be marked on exactly
those rows. Paths are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `POST /exports` | `{kind, from?, until?, station_id?, band?, mode?, qsl_sent?, contest_id?, q?, park?, length?, encoding?, unencodable?}` | `201 {batch, records, adif, file}` |
| `GET /exports` | | The batches, newest first |
| `GET /exports/{id}` | | The batch with `qsos: [{id, revision}]` |
| `POST /exports/{id}/apply` | | `{updated, skipped}`, as for bulk QSL changes |

A batch is `{id, kind, created_by, created_at, applied_at, qso_count}`. `kind`
is `adif`, `pota`, `eqsl`, `lotw` or `qsl_card`. The filters match as in
Search, with `qsl_sent` a list; `park` keeps one park of a `pota` export and is
`400` for other kinds. `encoding` is `utf-8` (default), `shift_jis`,
`windows-1252` or `ascii`. For `utf-8`, `length` is `codepoints`, `bytes` or
`utf16`, by default the server's `[adif] length`; for the others lengths are
the encoding's bytes and another `length` is `400`. `unencodable` says what to
do with a character the encoding cannot represent: `error` (default) is `400
unencodable` with the record number, the field and the character; `replace`
writes `?`, `entity` writes `&#NNN;` and `omit` leaves the field out. `adif` is
the whole file as text, oldest QSO first; `file` is its bytes in base64 for
`shift_jis` and `windows-1252`, and null otherwise. A `pota` export has one
record per park of a QSO, with `MY_SIG` and `MY_SIG_INFO` set. When nothing
matches, the response is `200` with `batch`, `adif` and `file` null and no
batch is recorded.

Applying an `eqsl`, `lotw` or `qsl_card` batch sets `EQSL_QSL_SENT`,
`LOTW_QSL_SENT` or `QSL_SENT` to `Y` and its sent date to the UTC day, on the
rows still at their recorded revisions. A batch is applied once; again is
`409`. `adif` and `pota` batches are records only, and applying them is `400`.

## ADIF Import

`POST /api/logbooks/{logbook_id}/imports?source=&station_id=&instrument_id=&format=&length=&encoding=&apply=`
takes the file itself as the body, up to 64 MiB. It works as the CLI
`import` does (see [decisions.md](../development/decisions.md), "ADIF Import"):

| Parameter | Meaning |
| --- | --- |
| `source` | `new` adds QSOs; `lotw`, `eqsl` or `card` merges confirmations into existing ones |
| `station_id` | The Station of new QSOs, required for `new`; for a merge, matches only its QSOs |
| `instrument_id` | The Instrument of new QSOs, baked in where a record has no value; `400` for a merge |
| `format` | `adi` (default) or `adx` |
| `length` | How an `.adi` file counts lengths, `codepoints`, `bytes` (of the file's encoding) or `utf16`; without it each non-ASCII value is read in the unit that reaches the next tag, trying the server's `[adif] length` first (bytes first for a file that is not UTF-8) |
| `encoding` | `utf-8`, `shift_jis`, `windows-1252` or `ascii`; without it the file is read as UTF-8 if it is valid UTF-8, else as the server's `[adif] fallback_encoding`. A file that is not valid in it is `400 not_utf8` or `400 not_encoding` |
| `apply` | `true` writes; without it the import only previews and writes nothing |

New QSOs get IDs derived from their records, so rerunning an import creates
only the records it has not created before. A file that cannot be read at all is
`400`; a record that cannot be read or checked is reported and skipped.

The response for `new` is
`{applied, encoding, lengths, read, created, existing, duplicates, outside_band, modes, invalid, rejects}`,
and for a merge
`{applied, encoding, lengths, read, not_confirmations, merged, already, unmatched, ambiguous, invalid, skipped, differences, rejects}`:

- `encoding` is the encoding the file was read in, and `lengths` the units the
  non-ASCII values of an `.adi` file were read in, such as `["bytes"]`; it is
  empty for ADX and for a file in ASCII.

- `duplicates` and `ambiguous` are `[{record, ids}]`, `outside_band` is
  `[{record, freq, band}]`, `modes` is `[{record, kind, ...}]` with the mode
  warnings of QSOs, `invalid` is `[{record, error}]` and `unmatched` is a list
  of record numbers, counted from 1 in file order.
- `skipped` lists the QSOs that changed between matching and writing, as in bulk
  QSL changes.
- `differences` counts, per field, the matched QSOs whose `DXCC`, `CQZ`, `ITUZ`,
  `STATE`, `CNTY`, `GRIDSQUARE` or `IOTA` differ from the confirmation; they are
  never applied.
- `rejects` is an ADIF file of the invalid, unmatched and ambiguous records, or
  null when there are none. It is UTF-8, with lengths in the `length` asked
  for, else in the server's `[adif] length`.

## Event Stream

`GET /api/logbooks/{logbook_id}/events?after=N` is a Server-Sent Events stream
of every change to the Logbook's QSOs, unfiltered; the client re-evaluates what
it shows. It starts after `Last-Event-ID` when the browser reconnects, else
after `after`, else at the Logbook's latest `updated_seq`. A `Last-Event-ID` or
`after` that is not an integer is `400`.

The first event is `hello`, without an `id`:

```text
event: hello
data: {"cursor_generation":"...","last_updated_seq":42,"after":40}
```

A client whose stored `cursor_generation` differs closes the stream, discards
its rows and cursor, and syncs again from 0. Then each changed QSO, deleted
ones included, is one `qso` event in `updated_seq` order, its `id` being the
`updated_seq`:

```text
event: qso
id: 41
data: {"cursor_generation":"...","qso":{...}}
```

A QSO changed several times since the cursor arrives once, in its current
state. A client syncs from scratch by reading `/changes` up to its
`last_updated_seq` H and then opening the stream with `after=H`; rows seen twice
are upserts. Comments are sent every 15 seconds to keep the connection open.

Writes through the server reach the streams at once; writes by the CLI in
another process reach them within 10 seconds. The stream ends when the caller
loses the Logbook (its Membership, its user or its token), its session expires
or is deleted, and when the server shuts down; the browser then reconnects
with `Last-Event-ID`, and a `404` or `401` stops it. Session checks do not renew
the session. At most one page of 500 QSOs is queued per stream. Even while
that queue is being sent, authorization is checked on server writes and every
10 seconds, and shutdown discards the queue immediately. At most 64 streams
are open per server; one more is `503`, and the client retries later.
