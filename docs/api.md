# Native API (Draft)

The API the web client uses, under `/api` on `PUBLIC_ORIGIN`. Bodies are JSON.
An error is `{"status": "error", "error": "<message>"}` with the HTTP status:
`400` for an invalid request or an expired passkey request, `401` when not signed
in or a passkey or registration link is refused, `403` for a cross-origin write or
a missing right, `404` for anything not found or not visible, `409` for a
conflict, `429` when too many passkey requests are in flight.

Only sign-in exists so far; QSOs, Stations and the SSE stream follow.

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
