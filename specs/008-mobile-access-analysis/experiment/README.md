# Experiment notes: iPhone over Tailscale

This folder holds the notes and screenshots of the throwaway experiment; nothing here ships (FR-009). The steps are in [../quickstart.md](../quickstart.md).

Run on 2026-10-26: personal Mac "macbook-delilah", tailnet `tail60414d.ts.net`, Tailscale macOS app 1.104.2, iPhone 15 Pro (iOS version not reported).

## Setup time (SC-005)

The user installed Tailscale on the Mac and the iPhone and enabled MagicDNS and HTTPS certificates in the admin console (DNS page), then ran two `tailscale serve` commands. The user reported the effort as minimal; it was not timed. Note: HTTPS certificates publish the tailnet and machine names in public Certificate Transparency logs.

Finding: `tailscale serve --https=443 <dir>` fails on the macOS app ("Path serving is not supported on macOS due to sandbox restrictions. To use Tailscale Serve on macOS, switch to the open-source tailscaled distribution."). The UI therefore has to be served by a local HTTP server (the daemon, per R4) and `serve` proxies to it.

Setup that worked:
- Preview daemon run directly with the same env as `scripts/preview.sh` (slot 9, API `127.0.0.1:7509`) and `DELILAH_UI_ORIGIN=https://<ts-name>`, because `preview.sh` hardcodes that variable to its own Vite URL.
- Throwaway Vite dev server on `127.0.0.1:1509` from `fixtures/mobile-vite.config.mjs` (git-ignored: root `../app`, `allowedHosts: [".ts.net"]`, HMR off, proxy `/api` to 7509 with `changeOrigin` and `ws`), started with `VITE_DELILAH_URL=https://<ts-name>` (same origin). `fixtures/node_modules` symlinked to `app/node_modules`.
- `tailscale serve --bg --https=443 http://127.0.0.1:1509`.
- Fixture project `signal` and backlog tasks created via the CLI.

## Q1 Loads over the tunnel (Wi-Fi and mobile data)

The UI loads on the iPhone over the tunnel and works (network not recorded; mobile data not confirmed). The layout is not responsive ("se ve raro"): expected, it is roadmap step 3. The user wants to design the phone layout separately with Claude Design.

Also verified from the phone: answering an agent works (task LIBRE-3 history: agent-asked 14:16:38, user-answered 14:17:41; US3), and creating a task (LIBRE-3) and starting LIBRE-2 works; the user finds the number of taps fine, not counted (US6).

## Q2 Host reaching the daemon; curl without Origin

Caveat: the no-Origin `curl` was run from the Mac itself via the tailnet name (through `tailscale serve`, so the same path as a tailnet device, but not from a second device), and the 200 was through the Host-rewriting Vite proxy.

`serve` forwards the original Host (`macbook-delilah.tail60414d.ts.net`). The unchanged preview daemon refused every API call with 403 "the Host must be a loopback address", with and without Origin: it fails closed today. Then, through a throwaway proxy that rewrites Host to loopback (simulating roadmap step 2 without step 1), `curl` with no Origin got 200 on `/api/v1/projects`. This confirms that accepting the tailnet Host without tokens gives the whole API to any tailnet caller (R3).

## Q3 Catch-up after 10 min locked

After about 10 min locked, the user says the view updated by itself ("diría que sí"). Catch-up time was not measured: partially verified.

## Q4 Add to Home Screen

Added to the Home Screen; it opens correctly as a web app. Push was not tested (needs roadmap step 4).

## Q5 Conversation and terminal at phone width

Conversation and terminal are readable on the phone ("se puede"). The layout itself is not adapted (see Q1).

## Q6 Phone terminal resizing the Mac's view

Not tested (Mac PTY resize when the phone opens the terminal): unverified.

## Screenshots

None taken.
