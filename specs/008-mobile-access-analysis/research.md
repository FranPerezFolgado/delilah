# Research: Mobile access analysis

Phase 0 of [plan.md](plan.md). Each section: **Decision / Rationale / Alternatives**.
Facts were checked by the experiment in [quickstart.md](quickstart.md); results are marked
"verified (Qn)", "partly verified", "unverified" or "from docs, unverified" (read from docs
or general knowledge, not tried). Claims without a source link are "from general knowledge,
unverified".

## Experiment outcome (2026-10-26)

Run on the personal Mac (Tailscale macOS app 1.104.2) and an iPhone 15 Pro (iOS version not
reported). Details: [experiment/README.md](experiment/README.md).

**What changed: R4 (and the text that depended on it in R1, R7 step 2 and the plan summary).**
The macOS Tailscale app refuses path serving ("Path serving is not supported on macOS due to
sandbox restrictions"). So `serve` cannot mount `app/dist` on the Mac, and the UI must be
served by a local HTTP server. **The daemon serving `app/dist` same-origin is now REQUIRED,
not optional.** No other decision R1-R9 is contradicted.

| Check | Result |
|---|---|
| Q1 loads and works on iPhone over the tunnel | Verified on Wi-Fi. Mobile data not confirmed (unverified). Layout not responsive (expected, step 3) |
| Q2 `serve` forwards the tailnet `Host`; today's daemon refuses | Verified: 403 "the Host must be a loopback address", with or without `Origin` (fails closed). With a throwaway proxy rewriting `Host` to loopback (simulates step 2 without step 1), curl with no `Origin` got 200: confirms R3's rule |
| Q3 catch-up after ~10 min locked | Partly verified: the view updated by itself (per the user); catch-up time not measured |
| Q4 Home Screen install | Verified: opens as a full-screen web app. EU claim: iOS version not reported, from docs, unverified |
| Q5 conversation and terminal readable | Verified |
| Q6 phone resizing the Mac's PTY | Unverified (Q6 not run) |
| US3 answer from phone | Verified: agent-asked, user-answered in ~1 min, first try |
| US6 create + start task | Flow verified, taps not counted (user finds it fine) |
| Push, deep link | Unverified until step 4 |
| SC-005 setup | Install app on both devices, enable MagicDNS + HTTPS in the admin DNS page, two `serve` commands. Not timed |

**Privacy note**: HTTPS certs for `*.ts.net` are logged in public Certificate Transparency
logs, so the tailnet and machine names become public (not the traffic). See R1 and R5.

Code facts (from the current tree):
- Daemon binds loopback only (default `127.0.0.1:7420`, `config.rs:6`); non-loopback `DELILAH_ADDR` is refused (`crates/delilah-daemon/src/config.rs:14,74`).
- Origin guard + CORS allow `tauri://localhost`, `http://tauri.localhost`, `http://localhost:1420`, plus `DELILAH_UI_ORIGIN` (`crates/delilah-daemon/src/api.rs:24-31`).
- `Host` must be a loopback name (guard `api.rs:244`, check `api.rs:264-267`, `is_loopback_host` `api.rs:274`). A request **without** `Origin` passes (`api.rs:255`; known problem, `docs/status.md:150`).
- The UI uses no Tauri commands; it finds the daemon via `VITE_DELILAH_URL` (`app/src/api.ts:2,5`).
- The daemon serves no static files; the UI is served by Vite (dev/preview) or the Tauri bundle.
- Live channels: events WS (`app/src/live.ts:42`), terminal binary WS (URL `api.ts:825`, opened at `TerminalPane.tsx:70`), chat live WS (`api.ts:861`), conversation polling with cursor (`api.ts:771`). Reconnect backoff 0.5→5 s (`live.ts:58-61`); on every (re)connect the client refetches all topics (`live.ts:46-49`).
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
| iPhone / Android | iPhone: PWA from Safari, push from iOS 16.4+. Android: same web app, push works in Chrome too (not the target, FR-010) | iPhone: Safari tab only, no push without the CA | Same as A | iPhone: needs AltStore/SideStore + a Mac or PC. Android: APK, no store needed | Same as A |
| Live updates / background | WebSocket live while the screen is on; drops on lock, refetch on return (R2). Push covers the gap | Same | Same | Same, plus native background limits | Same as A |
| Terminal | Same web terminal; read-only on the phone (R2) | Same | Same | Same | Same |
| Work Mac | Only if IT allows Tailscale | Usually blocked by Wi-Fi isolation / policy | Outbound tunnel; IT policy almost certainly forbids exposing a work machine publicly | — | Ruled out |
| Cost | Free (personal plan) | Free | Needs a domain on Cloudflare, usually paid (about a domain a year); Cloudflare plan free | Free (Apple ID), but a weekly refresh chore | Free |
| Store review | None for the web app; Tailscale iOS app is already on the App Store | None | None | None for sideloading (no store), but no store distribution either | Same as A |

Sources: sideload 7-day / 3-app limit ([AltStore FAQ](https://faq.altstore.io/altstore-world/your-altstore),
[XDA](https://www.xda-developers.com/how-to-sideload-apps-on-iphone/)); no Background Sync and
push only from the Home Screen on iOS ([MagicBell](https://www.magicbell.com/blog/pwa-ios-limitations-safari-support-complete-guide));
`serve` is tailnet-only and adds identity headers ([Tailscale serve](https://tailscale.com/kb/1312/serve));
`serve` cannot mount paths on the macOS app (verified in the experiment, see the outcome block;
[macOS variants](https://tailscale.com/kb/1065/macos-variants)). Everything else in this
table (Cloudflare Access, cost figures, Android push, LAN CA behaviour) is from general
knowledge, unverified. Privacy: A's HTTPS certs publish the tailnet and machine names in public
Certificate Transparency logs (reported from the experiment's setup; the logs were not inspected).

**Tab vs installed web app** (FR-001, US1), same option A:

| | Safari tab | Installed Home Screen web app |
|---|---|---|
| Install | None: open the URL | Share > Add to Home Screen (verified, Q4) |
| Full screen | No (Safari bars) | Yes, opens as a full-screen app (verified, Q4) |
| Push | **No** (iOS 16.4+ needs the Home Screen app) | Yes, iOS 16.4+ (from docs, unverified until step 4) |
| Live updates | Sockets drop when the tab is backgrounded or locked | Same; refetch on return (R2) |
| Needs manifest / service worker | No | Yes (step 4) |
| Use | Quick look, pairing, trying it | Daily use |

**Decision**: **A. Tailscale `serve`** for the personal Mac. The phone uses the same web UI as
a home-screen web app (PWA).

**Rationale**: it is the only option that is free, needs no developer account, gives valid
HTTPS (and therefore push and installability on iOS), and is never on the public internet.
It is the sketch already in `docs/vision.md`.

**Trade-offs accepted**: Tailscale on Mac and phone, a coordination service run by a third party
(metadata only), no access when the Mac sleeps, push only from the Home Screen app, a
read-mostly terminal on the phone, and no use on the work Mac unless IT agrees (R5).

**Against the vision sketch** (`docs/vision.md`, "Mobile, through Tailscale"):

| Sketch item | Outcome |
|---|---|
| `tailscale serve`, never the public internet | **Kept** |
| PWA served by the daemon | **Kept**, now required on macOS (R4; `serve` cannot mount paths there) |
| Token pairing | **Kept**, made concrete: QR + one-time code, per-device revocable token (R3) |
| Web push | **Kept**, content-free payloads, Home Screen app only (R2, R3.8) |
| Scope: see, answer, approve | **Kept** |
| Destructive actions on the desktop | **Kept**, and enforced server-side (R3.6) |
| Launching work (it was an open question) | **Changed**: allowed from the phone, with an explicit confirm |
| Which Mac / IT allows Tailscale | **Changed**: conditional; personal Mac first, work Mac only if IT approves (R5) |
| Bandeja-first scope, QR pairing (were "proposed") | **Decided** yes |
| iPhone or Android | **Decided**: iPhone first (FR-010) |

Dropped: nothing.

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
  refetches (decision #11); `live.ts` reconnects with backoff. Partly verified (Q3): after
  ~10 min locked the view updated by itself (per the user); catch-up time not measured.
- So "you are told while away" relies on **push**, not on the WebSocket.
- Spain is in the EU: Apple kept home-screen web apps in the EU after the iOS 17.4 reversal
  (from general knowledge, unverified; the iOS version was not reported in Q4). The install
  itself is verified (Q4): it opens as a full-screen web app.
- Android push in Chrome works (from general knowledge, unverified; not the target, FR-010).
- The terminal (xterm.js) works in iOS Safari but is hard to use without a hardware
  keyboard. Phone scope: **conversation view first**, terminal read-only.
- One viewer resizing the PTY would affect the Mac (`TerminalPane.tsx:24-30,80,84`: the focused pane
  sends `resize` and claims the size, and a phone would be focused). The phone must **never resize**. That needs a
  read-only client mode, and the daemon must also **drop `resize` messages from device tokens**
  so the rule does not depend on the UI.

## R3. Security model

**Key finding**: behind `tailscale serve`, every request reaches the daemon **from 127.0.0.1**,
so the loopback bind no longer says who is calling. Today the daemon still fails closed:
`serve` forwards the original `Host` (`<mac>.<tailnet>.ts.net`) and the `Host` guard
(`api.rs:264-267`) refuses it with 403, with or without `Origin` (verified, Q2). With a
throwaway proxy rewriting `Host` to loopback, a curl with no `Origin` got 200 (verified, Q2). The danger is created by **relaxing `Host`/`Origin`
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
   The redeem route is the only remote route that needs no token; it is rate limited and a
   code dies after 5 wrong tries. The token lives in the web app's own storage and travels
   in headers, never in a cookie, so a page on another site cannot ride it. Several devices
   (phone and tablet) can be paired at once, each with its own token and its own revoke.
3. **Origin**: add the tailnet origin (configured, not hardcoded) to the allow-list **only
   together with** a token requirement for that origin. Local app origins keep working
   without a token. Mobile off means no extra origin, no extra `Host`, no pairing route and unchanged behaviour:
   the exposure is today's (SC-006).
4. **Host**: accept the configured `*.ts.net` name only when mobile is on.
5. **Tailscale identity headers** (`Tailscale-User-Login`, set by `serve`, [docs](https://tailscale.com/kb/1312/serve)):
   use them as defence in depth only (log and match the owner), never as the only check. Any
   local process can reach 127.0.0.1 directly and forge them.
6. **Launch actions** (create task, start an agent, move a phase) are allowed from the phone
   with an explicit confirm step that shows, as text, what will run (project, prompt). There is
   no re-authentication in v1: the paired token plus the confirm is the bar. Passkey step-up
   is a later option (see Alternatives). Destructive actions are not offered on the phone at all
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
   (Tailscale admin), token lifetime 30 days sliding (an unused device expires by itself). Destructive actions are impossible from it.
10. **Reachability**: the daemon is never on the public internet; there is no exception. Only
    the user's tailnet devices reach the `serve` port, and only after a token check.
11. **Tailscale ACL**: recommend restricting port 443 of the Mac to the user's own devices.

**Alternatives**: Tailscale identity headers alone (forgeable locally); a single shared
password (no per-device revocation); mTLS client certs (iOS install friction); WebAuthn/passkeys
(strong but heavy for a single user; possible later as step-up for launch actions).

## R4. Serving the UI to the phone

**Decision** (**required** on macOS, changed by the experiment): the daemon serves the built UI
(`app/dist`) under `/` when mobile is on, **same origin** as the API, so `tailscale serve`
exposes one port and CORS is not needed for the phone. The build reads the API URL from the
page origin when `VITE_DELILAH_URL` is unset.

**Why required**: the macOS Tailscale app refuses path serving ("Path serving is not supported
on macOS due to sandbox restrictions", verified in the experiment;
[macOS variants](https://tailscale.com/kb/1065/macos-variants)), so `serve` cannot mount
`app/dist` and the UI must come from a local HTTP server.

**Alternatives**: `serve` mounting `app/dist` as a directory (**ruled out on the macOS app**;
might work with other Tailscale variants, unverified); a separate static server (one more
process, UI version can drift from the daemon's).

**Edge case: daemon down while the Mac is awake.** Because the daemon serves the UI, nothing
serves the page: `serve` has no backend on 127.0.0.1 and the phone gets an error page
(expected a 502 Bad Gateway; the exact page was not observed, unverified). An already
installed web app with a service worker (step 4) can still show its cached shell with an
offline/unreachable message; the first load or a Safari tab shows only the error. With a
separate static server the UI would load and then show "cannot reach the daemon". The phone
cannot start the daemon; this is the same as Mac asleep or off.

## R5. Work Mac

**Decision**: mobile access is **personal-Mac-first**. On the work Mac it is available **only
if IT approves Tailscale**. There is no fallback that keeps work content off third parties and
off the public internet: B is blocked by Wi-Fi isolation and policy, C exposes publicly.
Data leaving the work Mac, per option:

| Option | Leaves the Mac | Through |
|---|---|---|
| A. Tailscale `serve` | Coordination metadata (device names, IPs, keys). Content is end-to-end encrypted and usually direct; if no direct path exists it goes through a Tailscale relay (DERP), still encrypted | Tailscale |
| B. LAN | Nothing leaves the network | None (Wi-Fi devices can see it) |
| C. Cloudflare | All traffic, decrypted at Cloudflare | Cloudflare |
| D. Wrapper | Same as its transport | A/B/C |
| E. Funnel | All traffic, public | Tailscale relays |
| Push (A, step 4) | Payload with no content ("LIBRE-16 needs approval") | Apple push service |

Also for A: HTTPS certs for the Mac's `*.ts.net` name are logged in public Certificate
Transparency logs, so the tailnet name and machine name become public (the traffic does not).
On a work Mac the machine name would be visible too; pick a neutral name.

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
| Pair / revoke devices | | ✅ (pairing is started on the desktop) |

Phone vs desktop-only is enforced by the permission class of each route (R3.6), not by the UI.

## R7. Roadmap (FR-005): independently shippable steps

Each step: mobile off means today's behaviour (last column).

| # | Step | Size | Depends | Mobile off |
|---|---|---|---|---|
| 1 | **Device auth**: devices table, pairing code, one shared token layer for HTTP and the three sockets, permission class per route (destructive/config refused to device tokens), terminal `resize` dropped for device tokens, desktop "Devices" panel with revoke | M–L | — | No tailnet `Host`/`Origin` accepted, no pairing route; local callers need no token. Same as today. The devices table is additive |
| 2 | **Mobile switch**: config for the tailnet name; one switch turns on the tailnet `Host`/`Origin` **and** token enforcement for them (never one without the other); UI served same-origin by the daemon (required on macOS, R4: `serve` cannot mount paths there); setup guide for `tailscale serve` | S | 1 | Switch off: nothing is served or accepted beyond today's |
| 3 | **Phone layout**: breakpoint < 600 px; Bandeja + task list + conversation as stacked screens; terminal read-only, no resize | M | 2 (usable in a browser earlier) | Desktop widths are untouched (breakpoints only add a narrower one, `useWidth.ts:5-8`) |
| 4 | **Installable app + push**: manifest, service worker, Web Push (VAPID keys in Keychain), push on inbox events, content-free payloads | M | 2 | No device subscribed, so nothing is sent; the desktop keeps its native notifications |
| 5 | **Launch from phone**: create task / start / move phase screens with a confirm step (the server side already exists from step 1) | S | 1, 3 | Screens are reachable only from a paired device |

**Input for /feature (step 1)**: "Add per-device authentication to the Delilah daemon so remote
callers can be told apart and cut off: pairing by one-time QR code from the desktop, hashed
revocable bearer tokens (HTTP and the events, terminal and chat sockets, through one shared
layer), a permission class per route (read / answer / launch / destructive-config; the last is
refused to devices), `resize` dropped from devices on the terminal socket, and a Devices panel
on the desktop (name, paired at, last seen, revoke). Local callers (CLI, hooks, desktop app)
and behaviour with mobile off do not change. Context: research.md R3 and R7."

Step 1 makes remote callers safe. It does not change local ones: `docs/status.md:150` stays a
separate problem.

## R8. Experiment (throwaway, FR-009)

Run a branch preview, expose it with `tailscale serve` on the personal Mac, and open it from the
iPhone. This confirms R2/R3 facts **without changing shipped code**: launch the preview with
`DELILAH_UI_ORIGIN` set to the tailnet origin, and serve the Vite UI and the preview API
through `serve`. Steps in [quickstart.md](quickstart.md). Done on 2026-10-26: see the
experiment outcome at the top and [experiment/README.md](experiment/README.md). Facts not run
stay "unverified".

**SC-005 (setup effort)**: setup is one-time: install Tailscale on both devices, enable
MagicDNS + HTTPS in the admin DNS page, run two `serve` commands (minimal effort, not timed).
Pairing (step 1) adds about 1 minute per device (estimate, unverified), once.

## R9. Journeys on the phone (US3–US6)

For the recommended option (A: `tailscale serve` + Home Screen web app, steps 1–5 built).
Times are estimates for SC-004 and are confirmed when built. Experiment (2026-10-26): US3 and
US6 flows worked from the phone; taps and seconds were not counted, so the numbers below
stay estimates.

**US3. Answer a waiting agent** (target < 30 s).

| # | Action | Taps | Seconds |
|---|---|---|---|
| 1 | Push arrives: "LIBRE-16 needs approval" (no content, R3.8) | 0 | — |
| 2 | Tap it; Face ID unlocks; the Home Screen app opens on that inbox item (deep link) (unverified until step 4) | 1 | 3–5 |
| 3 | App reconnects and refetches (R2); the card shows the permission, question or plan | 0 | 2–5 (partly verified, Q3: time not measured) |
| 4 | Read the ask | 0 | 5–10 |
| 5 | Tap *Allow once* / *Deny*, or pick an option | 1 | 1–2 |

Total about 12–22 s, **2 taps** for a permission or a choice (estimate; flow verified from
the phone in the experiment: agent asked, user answered in ~1 min on the first try, taps not
counted). A free-text answer adds the
field tap, typing and *Send*: 3 taps, 25–40 s (over the target; the target is for the tap
answer). A plan approval is 2 taps plus reading time.
Works: a Home Screen app on iOS 16.4+ gets the push. Limits: no push in a Safari tab; no push
or answer when the Mac is asleep or off (the phone shows offline); the card loads only after
unlock.

**US4. Catch up after the lock** (10 min locked).
iOS suspends the page and the sockets drop (R2). On return `live.ts` reconnects (backoff up
to 5 s) and its `onopen` refetches every topic (`live.ts:46-49`), so no message is lost by
design. Estimate: 1–6 s to a current view (partly verified, Q3: after ~10 min locked the view
updated by itself, per the user; time not measured). Step 3 should also reconnect at
once on `visibilitychange` instead of waiting for the backoff timer (unverified, Q3).
Switching Wi-Fi to mobile data drops the socket the same way and follows the same path; it
also needs the Tailscale VPN to stay connected (unverified: Q1 was verified on Wi-Fi only). Limit: while locked there is
no live view, only push.

**US5. Follow a session** (conversation vs terminal).

| View | On the phone | Taps from the Home Screen app |
|---|---|---|
| Conversation | Works: cursor polling plus the chat live socket (`api.ts:771,861`), plain text, a stacked phone layout (step 3). Replying is possible | 3 (Tasks, task, session) |
| Terminal | Read-only: no typing, no resize. Hard to use without a keyboard (R2). Readable on the phone (verified, Q5) | 4 (adds the terminal tab) |

The Mac's view is not affected only if the phone never sends `resize`. Today it would, since
the focused pane claims the size (`TerminalPane.tsx:80,84`) (unverified, Q6 not run). So the phone
uses the read-only mode and the daemon drops `resize` from device tokens (step 1).

**US6. Launch work** (task + agent).

| # | Action | Taps |
|---|---|---|
| 1 | Open the app, tap New task | 2 |
| 2 | Write the idea (or pick a ticket) | 1 + typing |
| 3 | Tap Create | 1 |
| 4 | Tap Start agent; a confirm sheet shows the project and the prompt as text | 1 |
| 5 | Tap Confirm | 1 |

About 6 taps and 30–60 s, mostly typing (estimate: flow verified from the phone, taps not
counted, the user finds it fine). While the agent starts the phone shows the task and
its agent status from the events socket; it gets a push the first time the agent waits.
Limits: no re-authentication in v1 (R3.6); creating from a ticket needs the Mac's own ticket
setup to work; moving a phase is 1 tap plus the confirm. Delete, discard and settings are not
offered (R6).
