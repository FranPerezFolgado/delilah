# Quickstart: the iPhone-over-Tailscale experiment

Throwaway, on the **personal Mac** only. Nothing here changes shipped code (FR-009). Never use
port 7420 or the installed app; this runs against a preview.

## Prerequisites (user, ~10 min)

1. Install Tailscale on the Mac and the iPhone (App Store), same account.
2. In the Tailscale admin console: enable MagicDNS and HTTPS certificates.
3. Note the Mac's name: `tailscale status --self` → `<mac>.<tailnet>.ts.net`.

## Run

```bash
TS=<mac>.<tailnet>.ts.net
# 1. A preview daemon that accepts the tailnet UI origin (slot N → API 750N)
DELILAH_UI_ORIGIN="https://$TS" scripts/preview.sh --fresh      # note slot N
# 2. A static UI build that calls the API through the tunnel (port 8443)
(cd app && VITE_DELILAH_URL="https://$TS:8443" npx vite build --outDir /tmp/delilah-mobile-ui)
# 3. Expose both on the tailnet only (not funnel)
tailscale serve --bg --https=443  /tmp/delilah-mobile-ui
tailscale serve --bg --https=8443 http://127.0.0.1:750N
```

Open `https://$TS` in Safari on the iPhone (Wi-Fi **and** mobile data).

Undo: `tailscale serve reset`; stop the preview with Ctrl-C.

## Questions the experiment answers (recorded in research.md)

- **Q1** Does the UI load and list tasks over the tunnel, on Wi-Fi and on mobile data?
- **Q2** Which `Host` reaches the daemon? If requests are refused with "the Host must be a
  loopback address", `serve` forwards the tailnet name. Also check from another tailnet device:
  `curl https://$TS:8443/api/v1/tasks` with no `Origin`. If it returns data, R3's
  unauthenticated-access finding is confirmed.
- **Q3** Lock the iPhone for 10 min, unlock: does the view catch up by itself, and how fast?
- **Q4** Add to Home Screen: does it open full-screen? (Push is not tested; it needs step 4.)
- **Q5** Conversation and terminal views at phone width: what is unusable? (Screenshots go to
  `specs/008-mobile-access-analysis/experiment/`, not the main thread.)
- **Q6** With a session open on the Mac, does opening its terminal on the phone resize the
  Mac's view?
