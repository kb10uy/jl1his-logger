# Native API (Draft)

The API the web client uses, under `/api` on `PUBLIC_ORIGIN`. Bodies are JSON.
An error is `{"status": "error", "error": "<message>"}` with the HTTP status:
`400` for an invalid request or an expired passkey request, `401` when not signed
in or a passkey or registration link is refused, `403` for a cross-origin write or
a missing right, `404` for anything not found or not visible, `409` for a
conflict, `412` when `If-Match` names another revision, `415` / `422` for a body
that is not the expected JSON, `428` when `If-Match` is missing, `429` when too
many passkey requests are in flight. Some errors add fields next to `error`, such
as `current` on a revision conflict.

Sign-in, reference versions, Logbooks, Members, Stations and QSOs exist so far;
tokens, bulk QSL changes, export batches, ADIF and the SSE stream follow.

## Sign-in

WebAuthn options are the webauthn-rs JSON (`publicKey` with base64url binary
fields). A start returns a `ceremony` ID that its finish sends back with the
browser's `credential`; a ceremony is used once and expires after 5 minutes.

| Request | Body | Response |
| --- | --- | --- |
| `GET /api/me` | | The signed-in user `{id, callsign, role}`, renewing the cookie |
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

## Reference Data

`GET /api/reference` returns the versions of the reference data in use,
`[{kind, version, updated_at}]` with `kind` one of `cty`, `subdivisions` and
`parks`. It takes the session cookie only. Reference data is updated with the
CLI.

## Callers

The endpoints under `/api/logbooks` accept either the session cookie or a `native` API
token as `Authorization: Bearer <token>`. A token reaches only the Logbooks of
its scopes where its user still has a Membership; a `wl2` token is refused with
`401`. When `Authorization` is present the cookie is ignored. Sign-in and
passkey endpoints take the session cookie only.

A Logbook, Station or QSO the caller cannot reach is `404`, the same as one
that does not exist. A write reserved for owners is `403` for an operator.

## Logbooks

A Logbook is `{id, name, role, cursor_generation, last_seq, last_updated_seq}`,
where `role` is the caller's Membership, `owner` or `operator`.

| Request | Response |
| --- | --- |
| `GET /api/logbooks` | The caller's Logbooks, by name |
| `GET /api/logbooks/{logbook_id}` | One Logbook |

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

A Station is `{id, wavelog_id, name, callsign, fields, archived_at,
created_at, updated_at}`. `fields` holds the `MY_*` and other defaults baked
into new QSOs; `wavelog_id` is the Station's old Wavelog ID for `/wl2`. Paths
are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `GET /stations` | | The Stations, archived ones last, by name |
| `GET /stations/{id}` | | One Station |
| `POST /stations` | `{name, callsign, wavelog_id?, fields}` | `201` with the Station; owners only |
| `PUT /stations/{id}` | `{name, callsign, wavelog_id?, fields}` | The Station; owners only |
| `POST /stations/{id}/archive` | | The archived Station; owners only |
| `DELETE /stations/{id}/archive` | | The Station, no longer archived; owners only |
| `GET /stations/{id}/rewrite?from=&until=` | | `{count}` of past QSOs a rewrite would change; owners only |
| `POST /stations/{id}/rewrite` | `{from?, until?, count}` | `{count}` rewritten; owners only |

The callsign is upper-cased and field names are read ignoring case; an empty
value leaves the field out. `CALL`, `QSO_DATE`, `TIME_ON`, `BAND`, `MODE` and
`STATION_CALLSIGN` cannot be defaults. A `wavelog_id` another Station has is
`409`.

Editing a Station never changes past QSOs. A rewrite does: it covers the
Station's live QSOs with a QSO time in `[from, until)` (RFC 3339 times, both
optional) and sets `STATION_CALLSIGN` and every field of the Station, leaving
other fields alone. Its `count` must equal what a preview returns now, or it is
`409` and nothing changes.

## QSOs

A QSO is:

```json
{
  "id": "019a0a4e-6f2b-7c3d-8e4f-0123456789ab",
  "station_id": "...",
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
`CALL`, `QSO_DATE`, `TIME_ON`, `BAND` and `MODE` are required. A deleted QSO
keeps its fields and has `deleted_at` set. `created_by_callsign` and
`updated_by_callsign` are those users' current callsigns.

Paths below are relative to `/api/logbooks/{logbook_id}`.

| Request | Body | Response |
| --- | --- | --- |
| `POST /qsos` | `{id, station_id, fields}` | `201 {qso, warnings}`; `200 {qso, warnings: []}` with the existing row when the ID is known |
| `GET /qsos/{id}` | | The QSO, deleted or not |
| `PUT /qsos/{id}` with `If-Match` | `{station_id, fields}` | `{qso, warnings}` |
| `DELETE /qsos/{id}` with `If-Match` | | The deleted QSO; deleting a deleted QSO returns it unchanged |
| `GET /qsos?...` | | `{qsos, next}`, see Search |
| `GET /changes?after=N&limit=L` | | `{cursor_generation, last_updated_seq, qsos, has_more}` |
| `GET /duplicates?call=&band=&mode=&qso_date=&time_on=&except=` | | `{qsos}` |

`id` is a UUID v7 the client generates; anything else is `400`. Creating
fills `STATION_CALLSIGN` and the Station's `MY_*` defaults, then `OPERATOR` from
the caller's callsign, for fields the request leaves out. An `id` that belongs
to another Logbook is `409`.

Every response carrying one QSO has an `ETag` of its revision in quotes, such
as `"3"`. `PUT` and `DELETE` require `If-Match` with the revision they are based
on, in the same form; without it they are `428`, and a weak tag, `*` or several
tags are `400`. Another revision is `412`, and a `PUT` on a deleted QSO is
`409`; both carry the current QSO as `current`, so the client merges and retries
with its revision.

`PUT` replaces the whole QSO: `fields` is the complete new set, Station
defaults are not baked in again, and a field left out is removed.

`warnings` never refuse a write:

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `possible_duplicate` | `ids` | Live QSOs with the same call, band and mode, ignoring case, within 15 minutes |
| `frequency_outside_band` | `freq`, `band` | `FREQ` is outside `BAND` |

`/duplicates` runs the same check for a QSO being entered and returns the
matching QSOs by QSO time; `except` leaves out the QSO being edited.

### Changes

`/changes` returns the QSOs with `updated_seq > after` (default 0), deleted ones
included, by `updated_seq`, at most `limit` (default and maximum 1000). While
`has_more` is true, the next request continues from the `updated_seq` of the
last row. The response is read from one snapshot: `last_updated_seq` is the
Logbook's latest `updated_seq` in it, and no row goes beyond it. A client whose stored `cursor_generation` differs discards its
cursor and starts from 0.

### Search

`GET /qsos` returns live QSOs, newest first by QSO time, at most `limit`
(default 100, maximum 1000). Each filter is optional:

| Parameter | Matches |
| --- | --- |
| `from`, `until` | QSO time in `[from, until)`, as RFC 3339 times |
| `station_id` | The Station |
| `band`, `mode` | Ignoring case |
| `call` | The call exactly, ignoring case |
| `call_base` | The call without prefixes and suffixes (`JA1XYZ` matches `JA1XYZ/P`); not together with `call` |
| `qsl_sent` | Comma-separated `QSL_SENT` values ignoring case; an empty value matches an absent field |
| `contest_id` | `CONTEST_ID`, ignoring case |

`next` is an opaque cursor for the following page, passed back as `cursor`, or
`null` on the last page.
