# Quickstart: check the fix

1. Unit and e2e tests:
   ```bash
   cargo test -p delilah-agents claude::
   cargo test -p delilah-daemon launch_flags preferences
   cargo test -p delilah-e2e --test chat_permissions
   ```
2. A preview (never the installed app):
   ```bash
   scripts/preview.sh --fresh
   ```
   - In Ajustes, check that "Permisos de los agentes" shows **Auto**.
   - Register a fixture repo (`scripts/make-fixture-parent.sh`).
   - Start a task in chat mode and ask the agent to create a file, `sed -i` it, run a
     repo script and commit. No «¿Permitir?» card should appear.
   - Switch the setting to «Por defecto de Claude», relaunch the session (switch the
     runtime) and repeat. The cards come back, which is today's behaviour.
3. The spike, against the real CLI (it spends a few tokens):
   ```bash
   cargo run -p delilah-agents --example claude_stream_spike -- routine --permission-mode auto
   cargo run -p delilah-agents --example claude_stream_spike -- routine
   ```
   Compare the `can_use_tool` counts in each run's `summary.txt`. The findings are in
   `docs/spikes/2026-10-26-chat-permission-mode.md`.
