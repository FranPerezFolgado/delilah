# Contract sketch: device auth (proposed for roadmap step 1 — not built here)

Additive to the API in `crates/delilah-daemon/src/api.rs` ↔ `app/src/api.ts`. Old clients
(app origins) keep working without a token.

| Method | Path | Caller | Result |
|---|---|---|---|
| POST | `/api/v1/devices/pairing` | app origin | `{code, qr_url, expires_at}` |
| POST | `/api/v1/devices/pair` | phone, with `{code, name}` | `{device_id, token}` (token shown once) |
| GET | `/api/v1/devices` | app origin | `[{id, name, created_at, last_seen_at, expires_at}]` |
| DELETE | `/api/v1/devices/{id}` | app origin | 204, revoked now |

Auth for device callers:
- HTTP: `Authorization: Bearer <token>`.
- WebSocket: browsers cannot set headers, so the token goes in `Sec-WebSocket-Protocol`
  (`delilah.v1, <token>`), checked at the upgrade by the same shared layer as HTTP (401 before
  upgrade). No first-message auth, so the terminal socket's existing protocol (the server sends
  the size first, then binary frames and JSON `resize`) is unchanged. A `resize` from a
  device token is ignored.
- Local vs remote: a loopback `Host` is local (no token, as today); the tailnet `Host` is remote
  and always needs a token, with or without `Origin`.
- Missing/invalid/revoked token on a remote request → 401. Destructive/config route with
  a device token → 403.
