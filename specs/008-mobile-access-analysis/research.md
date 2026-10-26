# Research: Mobile access analysis

Phase 0 of [plan.md](plan.md). Each section: **Decision / Rationale / Alternatives**.
Facts marked **[verify in X]** are confirmed by the experiment in [quickstart.md](quickstart.md)
before the docs are updated (task phase "Verify").

Code facts (from the current tree):
- Daemon binds loopback only; non-loopback `DELILAH_ADDR` is refused (`crates/delilah-daemon/src/config.rs:14,74`).
- Origin guard + CORS allow `tauri://localhost`, `http://tauri.localhost`, `http://localhost:1420`, plus `DELILAH_UI_ORIGIN` (`crates/delilah-daemon/src/api.rs:24-31`).
- `Host` must be a loopback name (`api.rs:240-283`). A request **without** `Origin` passes (known problem, `docs/status.md:150`).
- The UI uses no Tauri commands; it finds the daemon via `VITE_DELILAH_URL` (`app/src/api.ts:2,5`).
- The daemon serves no static files; the UI is served by Vite (dev/preview) or the Tauri bundle.
- Live channels: events WS (`app/src/live.ts:42`), terminal binary WS (`api.ts:825`), chat live WS (`api.ts:861`), conversation polling with cursor (`api.ts:771`). Reconnect backoff 0.5→5 s (`live.ts:60`).
- Layout breakpoints are desktop-only (`app/src/useWidth.ts:5-8`: 1300/1200/960 px).

---

## R1. How the phone reaches the Mac

**Options compared** (iPhone, no paid developer account):

| | A. Tailscale `serve` | B. Same Wi-Fi (LAN) | C. Cloudflare Tunnel + Access | D. Sideloaded native wrapper | E. Tailscale `funnel` |
|---|---|---|---|---|---|
| Paid account / store | No (Tailscale personal plan free; iOS app is on the App Store) | No | No (free plan) | Free Apple ID; app expires every **7 days**, max **3** sideloaded apps; needs AltStore/SideStore refresh | No |
| Third parties | Tailscale coordination (keys, metadata); traffic is end-to-end WireGuard, usually direct | None | Cloudflare terminates TLS and sees all traffic | Same as the transport it uses (A/B/C) | Tailscale relays public traffic |
| Reachable from | Own tailnet devices only, any network | Same Wi-Fi only; client isolation on corporate/hotel Wi-Fi blocks it | The public internet, behind Cloudflare Access login | — (a client, needs A/B/C) | **The public internet** |
| HTTPS (needed for service worker, push, install) | Yes, valid cert for `*.ts.net` | No: needs a self-signed CA installed on the iPhone | Yes | n/a | Yes |
| Push to iPhone | Yes, as a home-screen web app (iOS 16.4+) | Only with the self-signed CA trusted; fragile | Yes | Native push needs a paid account → **no** | Yes |
| Setup | ~10 min: install on both, enable HTTPS certs, one `serve` command | Bind change + CA on phone | Domain on Cloudflare, `cloudflared`, Access policy | Xcode/AltServer, refresh weekly | Like A |
| Mac asleep/off | Unreachable; page shows offline | Same | Same, Cloudflare shows 502 | Same | Same |
| Work Mac | Only if IT allows Tailscale | Usually blocked by Wi-Fi isolation / policy | Outbound tunnel; IT policy almost certainly forbids exposing a work machine publicly | — | Ruled out |

**Decision**: **A. Tailscale `serve`** for the personal Mac. The phone uses the same web UI as
a home-screen web app (PWA).

**Rationale**: it is the only option that is free, needs no developer account, gives valid
HTTPS (and therefore push and installability on iOS), and is never on the public internet.
It is the sketch already in `docs/vision.md`.

**Alternatives ruled out**:
- **B (LAN):** no valid HTTPS without a home-made CA on the phone; does not work outside home;
  needs the daemon to bind beyond loopback, which widens exposure to every Wi-Fi device.
- **C (Cloudflare):** puts the daemon on the public internet behind a login, and a third party
  sees the traffic. It contradicts "never the public internet" and is wrong for work content.
- **D (sideloaded wrapper):** the 7-day expiry and the 3-app cap make it a weekly chore, and it
  gets no native push without a paid account. It adds nothing a home-screen web app lacks.
- **E (Funnel):** public exposure; ruled out by FR-003.

## R2. iPhone home-screen web app: what works

**Decision**: the phone client is the existing React UI, installed to the Home Screen (with a
web app manifest and a service worker), with a phone layout.

**Facts**:
- Web Push works on iOS 16.4+ **only** for web apps added to the Home Screen and launched
  from there, not in a Safari tab ([MagicBell](https://www.magicbell.com/blog/pwa-ios-limitations-safari-support-complete-guide), [Monterail](https://monterail.com/blog/pwa-for-apple-ios)). Permission must be requested from a user tap.
- No Background Sync. When the screen locks or the app is backgrounded, iOS suspends the page
  and **WebSockets drop**. On return, the UI must reconnect and refetch. Delilah's design
  already fits: live messages announce topics only and a client that missed messages just
  refetches (decision #11); `live.ts` reconnects with backoff. **[verify in Q3]** time to
  catch up after 10 min locked.
- So "you are told while away" relies on **push**, not on the WebSocket.
- Spain is in the EU: Apple kept home-screen web apps in the EU after the iOS 17.4 reversal.
  **[verify in Q4]** on the user's iOS version.
- The terminal (xterm.js) works in iOS Safari but is hard to use without a hardware
  keyboard. Phone scope: **conversation view first**, terminal read-only.
- One viewer resizing the PTY would affect the Mac (`TerminalPane.tsx:24-30`: the focused pane
  sets the size, and a phone would be focused). The phone must **never resize**. That needs a
  read-only client mode, and the daemon must also **drop `resize` messages from device tokens**
  so the rule does not depend on the UI.

## R3. Security model

**Key finding**: behind `tailscale serve`, every request reaches the daemon **from 127.0.0.1**,
so the loopback bind no longer says who is calling. Today the daemon still fails closed:
`serve` is expected to forward the original `Host` (`<mac>.<tailnet>.ts.net`) **[verify in Q2]**,
and the `Host` guard (`api.rs:262`) refuses it. The danger is created by **relaxing `Host`/`Origin`
to accept the tailnet name**. From then on, a tailnet caller with no `Origin` (curl, a script)
would get the whole API with no authentication.
**Rule:** the daemon accepts the tailnet `Host` only while token enforcement for it is active.
This is checked in code (one config switch turns on both, and there is no way to get one
without the other), not left to the order in which steps ship.

**Telling local from remote callers.** The CLI, agent hooks and the desktop app send no token
and must keep working. A request is treated as **local** (no token, today's rules) only if its
`Host` is a loopback name. Any request whose `Host` is the tailnet name is **remote** and
needs a valid device token, with or without `Origin`. Local processes keep full access: that
is the separate known problem in `docs/status.md:150`, which this design does not fix or
worsen.

**Decision**:
1. **Device tokens before any exposure.** Per-device bearer token, issued by pairing,
   stored hashed in the DB, sent as `Authorization` (HTTP) and as a `Sec-WebSocket-Protocol`
   value (browsers cannot set WS headers). Revocable from the desktop (list of devices: name,
   paired at, last seen). Remote requests (tailnet `Host`) with no valid token are refused. The
   check is **one shared layer/extractor** for HTTP and all three sockets (events, terminal,
   chat), not per-handler code.
2. **Pairing by QR from the desktop**: the desktop shows a one-time code (QR + 6 digits)
   valid for 5 minutes. The phone opens the URL and redeems the code once for a token.
3. **Origin**: add the tailnet origin (configured, not hardcoded) to the allow-list **only
   together with** a token requirement for that origin. Local app origins keep working
   without a token. Mobile off means no extra origin and unchanged behaviour (SC-006).
4. **Host**: accept the configured `*.ts.net` name only when mobile is on.
5. **Tailscale identity headers** (`Tailscale-User-Login`, set by `serve`, [docs](https://tailscale.com/kb/1312/serve)):
   use them as defence in depth only (log and match the owner), never as the only check. Any
   local process can reach 127.0.0.1 directly and forge them.
6. **Launch actions** (create task, start an agent, move a phase) are allowed from the phone
   with an explicit confirm step. Destructive actions are not offered on the phone at all
   (delete task/idea, discard workspace, force push, edit settings or hooks). The API refuses
   them for device tokens: the rule is enforced server-side, not just hidden in the UI. Each
   route gets a **permission class** (read / answer / launch / destructive-config) declared
   in the route table or as a layer, not as ad hoc checks. No such classification exists
   today. It ships **in step 1**, so device tokens are never all-powerful.
7. **Agent content** on the phone is rendered as text (as today, no HTML from agents). Links
   open in Safari, never as actions on the Mac.
8. **Lock screen**: push payload says only *what* waits ("LIBRE-16 needs approval"), not
   the content (no code, ticket text or diff). The content loads after unlock and a
   token-authenticated fetch.
9. **Stolen phone**: until revoked, it can see, answer and launch. Mitigations: iOS passcode,
   token revocation from the desktop (immediate), removing the device from the tailnet
   (Tailscale admin), token lifetime 30 days sliding. Destructive actions are impossible from it.
10. **Tailscale ACL**: recommend restricting port 443 of the Mac to the user's own devices.

**Alternatives**: Tailscale identity headers alone (forgeable locally); a single shared
password (no per-device revocation); mTLS client certs (iOS install friction); WebAuthn/passkeys
(strong but heavy for a single user; possible later as step-up for launch actions).

## R4. Serving the UI to the phone

**Decision** (preferred, optional; the spec does not require it): the daemon serves the built UI (`app/dist`) under `/` when mobile is on,
**same origin** as the API, so `tailscale serve` exposes one port and CORS is not needed for
the phone. The build reads the API URL from the page origin when `VITE_DELILAH_URL` is unset.

**Alternatives**: `tailscale serve` mounting `app/dist` as a directory next to the proxied API
(works, but two mounts to configure, and the UI version can drift from the daemon's); a
separate static server (one more process).

## R5. Work Mac

**Decision**: mobile access is **personal-Mac-first**. On the work Mac it is available **only
if IT approves Tailscale**. There is no fallback that keeps work content off third parties and
off the public internet: B is blocked by Wi-Fi isolation and policy, C exposes publicly.
Data leaving the work Mac with A: Tailscale coordination metadata (device names, IPs, keys).
Traffic is end-to-end encrypted. Push payloads go through Apple's push service, which is why
they carry no content (R3.8).
**Action for the user**: ask IT before installing anything on the work Mac. The analysis does
not depend on the answer.

## R6. Mobile scope (FR-012)

| Area | Phone | Desktop only |
|---|---|---|
| Bandeja (inbox): permissions, questions, plan approvals | ✅ answer | |
| Tasks list, phase, agent status, PRs | ✅ view | |
| Conversation of a session | ✅ view + reply | |
| Terminal of a session | 👁 read-only, no resize | type, resize |
| New task (idea / ticket), start agent, move phase | ✅ with confirm | |
| Delete, discard workspace, force operations, settings, hooks, project config | | ✅ |
| Editor, diff review, spec editing | | ✅ (later, maybe read-only) |

## R7. Roadmap (FR-005): independently shippable steps

Each step: mobile off means today's behaviour.

| # | Step | Size | Depends |
|---|---|---|---|
| 1 | **Device auth**: devices table, pairing code, one shared token layer for HTTP and the three sockets, permission class per route (destructive/config refused to device tokens), terminal `resize` dropped for device tokens, desktop "Devices" panel with revoke | M–L | — |
| 2 | **Mobile switch**: config for the tailnet name; one switch turns on the tailnet `Host`/`Origin` **and** token enforcement for them (never one without the other); UI served same-origin by the daemon, or mounted by `serve` (R4); setup guide for `tailscale serve` | S | 1 |
| 3 | **Phone layout**: breakpoint < 600 px; Bandeja + task list + conversation as stacked screens; terminal read-only, no resize | M | 2 (usable in a browser earlier) |
| 4 | **Installable app + push**: manifest, service worker, Web Push (VAPID keys in Keychain), push on inbox events, content-free payloads | M | 2 |
| 5 | **Launch from phone**: create task / start / move phase screens with a confirm step (the server side already exists from step 1) | S | 1, 3 |

Step 1 makes remote callers safe. It does not change local ones: `docs/status.md:150` stays a
separate problem.

## R8. Experiment (throwaway, FR-009)

Run a branch preview, expose it with `tailscale serve` on the personal Mac, and open it from the
iPhone. This confirms R2/R3 facts **without changing shipped code**: launch the preview with
`DELILAH_UI_ORIGIN` set to the tailnet origin, and serve the Vite UI and the preview API
through `serve`. Steps in [quickstart.md](quickstart.md). If the user prefers not to install
Tailscale yet, the facts stay marked "from docs, unverified".
