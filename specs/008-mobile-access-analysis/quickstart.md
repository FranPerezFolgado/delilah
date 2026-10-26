# Quickstart: the iPhone-over-Tailscale experiment

Throwaway, on the **personal Mac** only. Nothing here changes shipped code (FR-009). Never use
port 7420 or the installed app; this runs against a preview.

## Prerequisites (user, ~10 min)

1. Install Tailscale on the Mac and the iPhone (App Store), same account.
2. In the Tailscale admin console: enable MagicDNS and HTTPS certificates.
3. Note the Mac's name: `tailscale status --self` → `<mac>.<tailnet>.ts.net`.

## Run

`tailscale serve <dir>` does not work on the macOS app (sandbox), so the UI is served by a local
HTTP server and a single `serve` proxies to it. Same origin for UI and API.

```bash
TS=<mac>.<tailnet>.ts.net
# 1. Preview daemon run directly (env as in scripts/preview.sh, slot N → API 750N), with
#    DELILAH_UI_ORIGIN="https://$TS" (preview.sh hardcodes it, so it can't be used as is)
# 2. Throwaway Vite dev server on 127.0.0.1:150N from a git-ignored fixtures/mobile-vite.config.mjs:
#    root ../app, allowedHosts [".ts.net"], hmr off, proxy "/api" → http://127.0.0.1:750N
#    (changeOrigin, ws). Symlink fixtures/node_modules → app/node_modules.
VITE_DELILAH_URL="https://$TS" npx vite --config fixtures/mobile-vite.config.mjs --host 127.0.0.1 --port 150N
# 3. Expose it on the tailnet only (not funnel)
tailscale serve --bg --https=443 http://127.0.0.1:150N
```

Expected: without a Host fix, the daemon answers 403 "the Host must be a loopback address"
(Q2). To see the UI work, a Host-rewriting proxy is needed (the Vite proxy's `changeOrigin`
does it); never leave that exposed.

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
