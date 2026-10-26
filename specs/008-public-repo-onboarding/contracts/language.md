# Contract: UI language resolution

The rule is the same in the web UI (`app/src/i18n.ts`) and the desktop shell's native menu
(`app/src-tauri/src/daemon.rs`).

Inputs:
- `pref`: the daemon's `language` preference (`GET /api/v1/preferences/language`), which may
  be missing, `null`, `"auto"`, `"es"`, `"en"`, or any other JSON value. The API is unchanged.
- `system`: the UI uses `navigator.language`; the shell uses the first entry of
  `defaults read -g AppleLanguages`. Either may be missing or empty.

Resolution:

| `pref` | `system` | Result |
|---|---|---|
| `"es"` | any | `es` |
| `"en"` | any | `en` |
| anything else (missing, `"auto"`, unknown) | starts with `es`, any case (`es`, `es-ES`, `es-419`, `ES`) | `es` |
| anything else | anything else, including missing or empty | `en` |

Effects:
- `document.documentElement.lang` matches the result from the module's first evaluation
  (before the preference loads, resolved with `pref` missing), and is updated when the
  preference arrives.
- `app/index.html` declares `lang="en"` as its static value.
- Dates use `es-ES` for `es` and `en-GB` for `en`, as today.
- The Settings control still offers Auto / Español / English and stores `"auto" | "es" | "en"`.
