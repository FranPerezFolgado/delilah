# Data model (proposed, for roadmap step 1 — not built in this feature)

The analysis proposes these entities so a later feature can specify them. No migration is
written here (FR-009).

## Device

A phone or tablet the user paired.

| Field | Notes |
|---|---|
| id | |
| name | given by the user at pairing ("iPhone de Fran") |
| token_hash | hash of the bearer token; the token itself is shown once, to the device |
| created_at, last_seen_at | last_seen updated at most once a minute |
| expires_at | sliding, 30 days from last use |
| revoked_at | set from the desktop; a revoked device gets 401 on its next request |

## Pairing code

| Field | Notes |
|---|---|
| code | 6 digits + random secret in the QR |
| expires_at | 5 minutes |
| used_at | single use |

State: `issued → redeemed` (creates a Device) or `issued → expired`.

## Permission class (server-side, per route)

- **read**: allowed to device tokens.
- **answer**: allowed (inbox replies, permissions, approvals, chat input).
- **launch**: allowed, with the client confirm step.
- **destructive / config**: refused for device tokens (403), app origins only.

Step 4 (push) adds its own table of Web Push subscriptions per device; it is not part of step 1.
Lifetimes (5-minute codes, 30-day sliding tokens) are proposals to confirm when step 1 is specified.
