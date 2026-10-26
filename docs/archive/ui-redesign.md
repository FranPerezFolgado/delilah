# UI redesign v2: refactor notes

Context for whoever applies the redesign to `app/`. The visual source of truth is the
Claude Design canvas **«Delilah · Cabecera de tarea»**:
https://claude.ai/artifact/7V8faKddSQLgsCNjgnEcya. Every board has a `tema` tweak (oscuro /
claro). The design conventions behind the boards (tokens, components, words, mock data) are
carried over here, and the canvas CSS is in the [appendix](#appendix-base-css). When the canvas
and this file disagree, ask.

Canvas pages and boards (compare each step against the boards named in §8):

| Page | Boards |
|---|---|
| Cabecera de tarea | Main, Antes, Estados |
| Sistema v2 | Sistema, Lateral |
| Pantallas | Tablero, Bandeja, Tarea, Jira, ProyectoVista, Proyectos, Ajustes, BacklogTarea |
| Diálogos y estados | NuevaTarea, Eliminar, QuickOpen, Logs, TareaSetup, Desconectado, PrimerUso |
| Anchos y escala | Tarea1280, Tablero1280, Jira1024, EscalaLateral, EscalaJira |
| Being drawn now | Conversacion (project conversation + the Con Claude proposal step), Cargando (skeletons), AvisosGlobales (toasts, update banner, quota, autopilot pause reasons), TareaLibre (non-feature workflow + several Claude sessions), TareaHecha, TareaCaida (agent crashed) |

All boards use the same mock data (projects signal / delilah / fixture-parent, tasks LIBRE-1…15).
`scripts/seed-preview.sh` seeds a preview with similar states; it need not match the mock data
exactly, but cover the same states so the comparison is fair.

Status: applied (spec `007-ui-v2`, branch `feature/ui-v2`; see [status.md](../status.md) and decisions 68 to 78). This file stays as the brief behind it: the visual rules, the glossary and §9, the before / after record to refresh when the UI changes again.

## Why

A UI review (October 2026) found:
- The dark theme is too dark and too high-contrast at once: text #e7e8ea on #121316 (15:1),
  surfaces 1.03–1.04:1 apart, so panels melt into one black slab while text glares.
- About 8 different treatments for status, phase and counts, 13 font sizes, ~6 button families,
  and 198 inline `style={{` across 24 files.
- Cluttered headers, hidden or duplicated actions, and flows that send you away to answer.

## Scope

A full refactor: tokens and components first, then every screen. The decisions below were
taken with the owner, point by point; don't reopen them without asking.

## 0. How to work (repo rules that apply to every step)

- **One step of §8 = one branch and one PR**, ending with
  `scripts/bump-version.sh <level> --commit` (commit `chore: bump version to X.Y.Z`). Steps 1, 2
  and 6 are visual and behaviour-preserving: **patch**, unless they add a user-facing capability
  (the disconnected state, the Logs button) → **minor**. Steps 3–5 add features → **minor**. Say
  the level and why in the PR (CLAUDE.md, docs/development.md › Releasing).
- **Try everything in a preview** (`scripts/preview.sh`, `--fresh` for an empty state or after
  a migration was renumbered). Never restart, stop or use the installed daemon on 7420 or
  `scripts/dev-daemon.sh`.
- **Themes:** there is no in-app theme setting; the app follows the OS through
  `prefers-color-scheme`. Check light and dark with the browser pane's colour-scheme emulation
  (or the OS setting).
- **Checks per step:** `cd app && npx tsc --noEmit && npx vitest run`; UI e2e
  `scripts/e2e.sh ui`; `cargo test --workspace` (and `scripts/e2e.sh api`) when the daemon
  changes; `cargo test --manifest-path app/src-tauri/Cargo.toml` when the shell changes.
- **Docs:** update `docs/status.md`, the README (user-facing behaviour) and `docs/decisions.md`
  (the workspace default in §6b, the glossary) in the PR that changes them.
- **Tauri vs browser.** The UI also runs in a plain browser (`api.ts` uses no Tauri commands;
  `isDesktop` in `platform.ts`). Native-only parts: the titlebar (`titleBarStyle: Overlay`,
  `hiddenTitle`, `minWidth: 1024` in `tauri.conf.json`: keep every `data-tauri-drag-region` and
  room for the traffic lights), notifications and the Dock badge (unchanged, decision 21),
  the native menu, «Mostrar en Finder» (needs a new Tauri command; hide it outside the desktop).
  Say so instead of trying them in a preview.

## 1. Tokens (`app/src/styles.css`)

Replace the `:root` blocks (`styles.css:2-17` and the diff tokens at `237-240`) with the v2 set
from the appendix: `.dl` becomes `:root` (dark, the default) and `.dl.light` becomes
`@media (prefers-color-scheme: light) { :root { … } }`. Summary:

| Token | Dark | Light | Use |
|---|---|---|---|
| `--bg` | #202125 | #f6f6f4 | app background |
| `--side` | #1c1d20 | #efefec | sidebar, titlebar, statusbar |
| `--panel` | #26272c | #ffffff | panels, inputs, cards on bg |
| `--card` | #2c2d33 | #ffffff | cards inside panels |
| `--dialog` | #2a2b30 | #ffffff | dialogs, menus, popovers |
| `--hov` / `--sel` | #33353b / #3a3c43 | #ebebe8 / #e3e4e8 | hover / selected |
| `--bd` / `--bd2` | #33353b / #41434a | #e3e3df / #d2d2cd | subtle / control borders |
| `--tx-strong` / `--tx` | #e8e9ec / #d4d5d9 | #111214 / #26272b | text |
| `--tx2` / `--tx3` | #a6a8ae / #979aa1 | #55585e / #6b6e75 | secondary / tertiary |
| `--primary` / `--primary-hov` / `--primary-tx` | #d4d5d9 / #e8e9ec / #1c1d20 | #26272b / #111214 / #ffffff | primary button |
| `--focus` | #7aa7f0 | #2f6fd0 | focus ring |
| `--wait*` | #e8b45c, bg #463a22, tx #f0c987 | #c58a2a, #fbf0dc, #7a4d0b | «Te espera» only |
| `--run*` | #7aa7f0, #2a3550, #a9c6f7 | #2f6fd0, #e5eefc, #1d4f9c | «Trabajando» only |
| `--pause*` | #ab9ff5, #36324f, #c7befa | #6a55c9, #eeeafb, #4a3a9c | «En pausa» |
| `--ok*` | #74c287, #26402f, #a3dcb1 | #2f8a46, #e3f3e7, #1f6234 | «Hecha», checks ok |
| `--err*` | #ef8a83, #4a2b2a, #f5aaa4 | #c9433b, #fbe6e4, #9a2a23 | errors, danger |
| `--neutral*` | #8d8f96, #33353b, #c2c4c9 | #8a8d93, #ececea, #4a4d53 | Backlog, inactive |
| `--add-bg/-tx`, `--del-bg/-tx`, `--hunk-bg` | see appendix | | diff |
| `--term-bg` / `--term-tx` | #26272c / #c9cad0 | #ffffff / #26272b | xterm and CodeMirror |
| `--shadow`, `--scrim` | see appendix | (light scrim .42) | dialogs, menus, backdrop |

**Old → new.** The old tokens have no 1:1 successor; decide per use:

| Old (uses in `app/src`) | New |
|---|---|
| `--acc` (40), `--accbg` (9), `--acctx` (6) | by meaning: primary button → `--primary`/`--primary-tx`; selection or current item → `--sel` + `--tx-strong`; focus → `--focus`; active tab underline (`.etab.on`, `styles.css:305`) and Jira row (`.jrow.on`, `:427`) → `--tx-strong` / `--sel`; «needs you» → `--wait*`. Never amber by default. |
| `--elev` (8) | `--card` (inside panels) or `--dialog` (floating) |
| `--done` (16) | `--ok` / `--ok-tx` |
| `--term` (9) | `--term-bg` |
| `--pausebg` | `--pause-bg` |
| `--addbg` / `--delbg` / `--hunkbg` | `--add-bg` / `--del-bg` / `--hunk-bg` (+ the new `-tx`) |
| raw shadows and backdrop (`styles.css:111,113,179,187`) | `var(--shadow)`, `var(--scrim)` |

Colour rules:
- **One colour, one meaning.** The primary button is light grey (dark text) and stands out by
  lightness; **one primary per view**, disabled ones included. Amber (`--wait`) only means «te
  espera / needs you», never a button, tab or phase. Blue (`--run`) only means the task status
  «Trabajando» and the focus ring.
- **Other in-progress things are neutral**: a running CI check, the spec task in progress, a PR
  «Abierta» use `--tx-strong` text/border and a small neutral spinner, never blue.
- Phase bars and the active tab underline are neutral (`--tx-strong`).
- Log lines: INFO `--tx2`, WARN `--tx-strong` + the word WARN, ERROR `--err-tx`. Never amber.
- **Contrast.** `--tx3` only on `--bg`, `--side` or `--panel`; on `--card` and `--sel` use `--tx2`
  or stronger (4.5:1).
- Type scale: 19 (view title), 15 (section), 13.5 (base), 12 (secondary, labels); mono 12. Remove
  every other size. Font stack `-apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui`; mono
  `ui-monospace, 'SF Mono', Menlo`. Radii: 7 controls, 10 cards, 14 dialogs, 11 pills. Rows
  30–32 px, two-line rows ~52 px. Density: «algo aireada».
- Focus: `:focus-visible` 2 px `--focus` outline, offset 2. Links `--run-tx`.
- Scrollbars and checkboxes/radios are themed (appendix); never native white scrollbars.
- **Hard-coded theme to move onto tokens and make live:**
  - `TerminalPane.tsx:10-15`: the xterm `THEME` is a constant (#0c0d0f, amber cursor). Build it
    from `--term-bg/--term-tx` and update `terminal.options.theme` when the scheme changes.
  - `editor/codemirror.ts:14` reads `matchMedia` **once** at module load and uses
    `oneDarkHighlightStyle`; `:21-29` hard-codes amber selection and search. Listen for the
    media query change (reconfigure through a `Compartment`), add a light highlight style, and
    use neutral selection colours.

## 2. Components (consolidate)

| New | Replaces | Notes |
|---|---|---|
| `Button` variants: primary, secondary, ghost, danger; sizes 32 / 28 | `.btn`, `.btn-p`, `.btn-ghost`, `.btn-danger`, `.saving-on/off`, inline `color: var(--err)` (`ProjectsView.tsx:117,219`), `.btn-ghost c-err` (`TaskView.tsx:181`, `BacklogTaskView.tsx:117`) | danger always has a red border, never plain red text |
| `StatusBadge` (dot + text, tonal bg) | `.pill c-*` (`TaskScreen.tsx:618`), inline pills (`TaskView.tsx:233`, `ProjectView.tsx:88`), `.kcard-status`, `.tag` for status, `.pcount` | task status only; mapping in §3 |
| `InboxKind` (icon + text, no bg) | the kind icon + label in `InboxView.tsx:131-133` | Pregunta · Permiso · Aviso |
| `PrChip` (PR icon + #n + state) | `.pr-chip`, `.pr-badge` (`PrBadge.tsx`) | Borrador · Abierta · Checks fallan · Fusionada |
| `PhaseBars` (4 segments + label) | `PhaseBadge`, `.phase-pill`, `.stepper` | see §4 |
| `Toggle` (switch chip, `aria-pressed`) | `AutopilotMark` + «Activar autopiloto», `.saving-pill` + «Desactivar» | same component in header and Nueva tarea |
| `Segmented` | `.segs` / `.seg` pairs (header Chat \| Terminal, Jira, Settings, NewTaskDialog) | see Accessibility for the role |
| `OptionCard` (with ✓) | «Dónde trabaja» cards (`WherePicker.tsx`), kind cards | |
| `Row` (list/sidebar row, `aria-current`) | `.nav`, `.srow`, `.phead-main`, `.bhead`, `.inbox-task`, and their padding overrides (`styles.css:60,331,379,404,445`) | one height, one selected style |
| `Count` (neutral / hot) | `.nav .count`, `.nav .count.badge` (`styles.css:56,326`) | hot = amber only when something waits |
| `Skeleton` | «Sin eventos todavía» / «La rama aún no ha cambiado nada» shown while loading | empty text only after load |
| `Dialog` + `ConfirmDialog` | `DeleteTaskDialog`, inline delete/edit in project rows, project settings without `role="dialog"` | one confirm pattern; button order always Cancelar · Acción |
| `Menu` | context menu, ad-hoc dropdowns | |
| `AskCard` (shared) | `AskCard.tsx` in inbox + task callout, `PermissionCard.tsx` | answers inline everywhere (§5) |

Components that keep their job and only get restyled: `SessionChain`, `PreviewControl`,
`SetupBanner` (becomes the Setup failed panel, §5), `HandoffActions`, `ProposalCard`,
`UpdateNotice` (see the AvisosGlobales board), `OpenPrDialog`, `ToolCard`, `ConversationView`,
`ChangesView`, `DiffRows`, `FileTree`, `QuickOpen`, `LogsView`.

**Selection is one language:** `--sel` background + `--tx-strong` text through `aria-pressed`,
`aria-selected` and `aria-current`; option cards add a ✓. The only exception is tabs: a neutral
2 px underline.

Remove the inline `style={{`; what is layout goes to CSS classes, what is a token goes to
variables. Only truly dynamic values stay inline (e.g. `--tree-w` in `TaskScreen.tsx:653`).

Icons: inline stroke SVG (`Icon.tsx`), stroke 1.5, 16 viewBox, `aria-hidden` when decorative.
No emoji.

### Accessibility

- Real `<button>`, `<a href>`, `<input>` + `<label>`. Icon-only buttons need `aria-label`.
- `Dialog`: `role="dialog"`, `aria-modal="true"`, `aria-labelledby` its title, focus trapped,
  Esc closes, focus returns to the opener.
- `Menu`: `role="menu"` / `menuitem`, arrow keys, Esc, focus returns.
- Tabs keep `role="tablist"`/`tab` + `aria-selected`.
- `Segmented`: a single-choice group whose choice is a value (Nueva tarea origin, Agrupar por)
  keeps `role="radiogroup"` / `radio` + `aria-checked` (e2e already finds «Free task» as a
  radio); a pair of actions or view modes (Chat | Terminal) uses buttons with `aria-pressed`.
  Same look for both.
- Keyboard shortcuts on lists (Bandeja ↑↓, 1–3, ⌫) only when focus is not in a text field.

## 3. Vocabulary

Status has three axes, each with its own visual form, plus the session dot.

**Task status → `StatusBadge`** (real `TaskStatus` in `api.ts:7-16`):

| `TaskStatus.type` | Badge | Detail |
|---|---|---|
| (backlog task, no run yet) / `queued` | Backlog (neutral) | |
| `preparing-workspace` | Trabajando (run) | «Preparando el workspace» |
| `running` | Trabajando (run) | |
| `waiting-for-user` | Te espera (wait) | |
| `paused-by-limit` | En pausa (pause) | «se reanuda a las HH:MM» from `resume_at` |
| `done` | Hecha (ok) | |
| `failed` | Error (err) | `reason` in the tooltip / detail |
| `setup-failed` | Error (err) | cause in the tooltip; header shows «Sin iniciar» instead of bars |

**Inbox item kind → `InboxKind`** (real `InboxKind`, `api.ts:368-370`):

| `InboxKind` | Kind | «Limpiar avisos» removes it? |
|---|---|---|
| `permission` | Permiso | no (agent waiting) |
| `question`, `waiting` | Pregunta | no (agent waiting) |
| `error`, `pr-failed`, `pr-review`, `pr-blocked` | Aviso (blocking) | **no**: stays until resolved |
| `review`, `handoff`, `suggest-review`, `paused`, `failed`, `branch`, `autopilot`, `base-stale` | Aviso | yes («Quitar» one by one too) |

The blocking set is `BLOCKING` (`api.ts:504`); the hot counts use it.

**PR status → `PrChip`:** Borrador · Abierta (neutral, not blue) · Checks fallan · Fusionada.

**Session:** only a dot (wait / run / neutral), never the word «Inactiva».

**Words (glossary, decided):**
- **Iniciar** starts a task, new or from backlog. Never «Lanzar», «Empezar», «Arrancar»;
  «Sin empezar» → «Sin iniciar».
- **Detener** stops a session or agent (never «Parar»). Transcript line: «Detenida: detuviste a
  Claude».
- **Abrir …** for every navigation («Abrir el tablero», «Abrir la tarea», «Abrir cambios»),
  never «Ver …» / «Ir a …». When a board still shows «Ver …», the rule wins.
- **Recargar** (never Refrescar) · **Omitir** (never Saltar) · **Guardar en backlog** ·
  **Responder** (opens the answer box) / **Enviar** (sends it) · **Eliminar…** (always confirms
  in a dialog) · **Quitar** (removes an inbox aviso) · **Cancelar**.
- **Autopiloto** and **Ahorro** are the two toggles. Ahorro is described as «Una sesión de
  Claude nueva por fase, para gastar menos contexto» (decision 33); never «modelo y esfuerzo
  más baratos».
- No English in UI copy: «flujo feature» / «flujo libre» (not «workflow»), «Control remoto».
  Session names stay as the app names them («Claude principal»; «Main Claude» in English).
- Logs level filter: «Todo · Info · Advertencias · Errores» (never «Avisos», which is the inbox
  kind). Log lines themselves stay in English: they are data.

**i18n (`app/src/i18n.ts`, `app/src/i18n/en.ts`, decision 20).** Copy is Spanish inside
`t("…")` / `tn(n, one, many)`; the English lives in `en.ts` **keyed by the Spanish text**. Every
copy change (e.g. «Lanzar» → «Iniciar», «Empezar {key}» → «Iniciar {key}») needs the en.ts key
renamed and the old key deleted, or the English UI falls back to Spanish. Run the dev build in
English: missing keys log `[i18n] sin traducir`. The UI e2e runs in English, so its labels
(«Start», «New task», «Free task») follow these renames.

Finish coverage. Found untranslated in the Spanish UI: «sample data», «Filter tickets…»,
«Connection», «Assigned/Open/All», «Analyse with Claude», «Search 4 files…», «REMOTE CONTROL»;
in the English UI: «0 archivos». «Jira» (`Sidebar.tsx:142`) is a product name and needs no key.
History shows raw slugs («setup-failed»): map every `EventRecord.event.type` to a human
sentence.

## 4. Phases

From `app/src/phases.ts`: 4 phases (Especificación, Plan, Implementación, Revisión). Backlog
and Hecha are not phases. Display: `PhaseBars` + label, always both: «Plan · 2/4»; in
Implementación add the spec progress «· 6/14 tareas» (`HeaderPhase.done/total`,
`phases.ts:29-35`). Only the 960 px header and the 56 px rail may drop the label (it goes to the
tooltip). The phase popover lists the 4 phases.

- **Hecha has no bars.** A done task shows the «Hecha» badge, the PR chip («#9 Fusionada») and
  a relative date («hace 2 días»). Don't use `.bars i.ok`.
- **Setup failed** shows «Sin iniciar» instead of bars.
- Board: Backlog + 4 phases + Hecha; an empty phase column folds to a 40 px strip.

## 5. Screens

### Shell (`App.tsx`, `StatusBar.tsx`)
- Sizes: titlebar 38, statusbar 28, sidebar 248 (rail 56).
- Titlebar breadcrumb: «Delilah / <vista>»; on a task «Delilah / signal / LIBRE-5». Today
  `App.tsx:266-268` shows only «/ KEY»; keep `data-tauri-drag-region` on every part.
- Statusbar: «● Conectado» · «3 trabajando · 1 te espera · 1 en pausa · 1 con error» · usage
  («Claude 5 h 82 % · Semana 35 %») · a **Logs** button (today Logs is only in the native menu).
  The counts are buttons that open the board filtered by that status; «con error» uses
  `--err-tx`. Today «Agentes trabajando 0» (`StatusBar.tsx:15`) reads wrong when tasks wait.
- **Daemon disconnected** state (new): err dot + «Sin conexión con delilahd · reintentando»,
  banner on top of main with «Reintentar ahora» and how to start it, content greyed and actions
  disabled, «Datos de hace N min».
- Errors are shown once, in human words, with an action. Loading shows skeletons, never «vacío».

### Sidebar (`Sidebar.tsx`)
- Nav: Bandeja (hot count = blocking items), Tareas (count), Jira, Proyectos. Shortcuts move
  to tooltips (today kbd chips sit in different places, `Sidebar.tsx:134-151`).
- «Proyectos» header with **«+» = «Añadir proyecto»** (everywhere). Today's «+ Tarea» nav
  button (`Sidebar.tsx:151`) goes: new task is the board header primary, ⌘N, and the
  per-project hover «+» («Nueva tarea en este proyecto»).
- Per project, keep everything it has today: header (chevron, name, hover actions «Preguntar a
  Claude» and «Nueva tarea en este proyecto»), «Ideas · N» row, project conversation rows (dot
  on the left like tasks), task rows, «Backlog · N» (a collapsible button), «N hechas»,
  «Sin proyecto» group. Ajustes at the bottom, next to a «Plegar barra lateral» icon button.
- Task rows: status dot + **title first** on one line; ID only on hover/active (today titles cut
  to ~10 chars). Tooltip: ID · title · status.
- Accessible names: project buttons announce the full path today; use the name.
- Collapses to a 56 px icon rail with **⌘\\** (a new `ActionId` in `shortcuts.ts`, so it appears
  in Ajustes › Atajos) and automatically below ~1300 px.
- At scale: first 6 active tasks per project + «Ver N más», a «Filtrar tareas» field.

### Tareas / board (`BoardView.tsx`)
- Flexible columns, empty phases folded, project filter (Todos / per project), primary «Nueva
  tarea». Cards: ID, status badge (always text, never icon-only), 2-line title, project, mini
  phase bars (none for Backlog/Hecha), PR chip, autopilot marker. Waiting cards are not
  highlighted with borders.
- First run (new, board PrimerUso): checklist «Añadir un proyecto» → «Conectar Jira (opcional)»
  → «Crear tu primera tarea», each marked when done; «Omitir» for Jira.
- At 1280: columns min 220 px with horizontal scroll.

### Bandeja (`InboxView.tsx`)
- Two panes: list (380) + detail. Relative times («hace 4 min») instead of absolute stamps.
- Item kinds and what «Limpiar avisos» removes: §3 table. New non-blocking Avisos: turn
  finished, PR opened.
- AskCard actions by kind: Pregunta → numbered options + free reply + Enviar (⌘↵); Permiso →
  command in a mono block + «Permitir una vez» / «Permitir siempre en esta tarea» / «Denegar»,
  no free text; Aviso → its action («Abrir PR», «Abrir cambios», «Abrir la tarea») + «Quitar»
  when it is not blocking. A hint line says the keys only work outside text fields.
- «Limpiar avisos» removes only non-blocking Avisos, with a Deshacer toast.
- Keys: ↑↓ move, 1–3 choose, ⌫ removes an aviso; only when focus is not in a text field.
- At scale: group items by task (with a count). Empty: «Nada te espera».
- Below ~1200 px the detail opens as an overlay.

### Task screen (`TaskScreen.tsx`)
- **Header**, left to right: key (mono) · **editable title** (click → input; after a rename a
  muted line «Antes: <old> · Restaurar», as today with `previous_title`, `:594-599`) · status
  badge | PhaseBars | **Chat | Terminal** segmented | **Autopiloto** and **Ahorro** toggles |
  **Preview** control (`PreviewControl`: «Preview» off / «Preview :1503» with an ok dot when
  on, menu Abrir · Detener) | tree toggle.
  - Chat | Terminal applies to the session of the open tab; hidden on the Tarea tab and on
    plain terminal tabs. It already exists in the tab row (`:708-728`, `.view-switch`); move it
    to the header. Two concepts behind it, keep them apart: the **view** of a PTY session
    (chat transcript vs xterm, client-side, `chooseView`) and the **runtime** of the task
    (`pty`/`stream`, `chat_mode`, changed with `setRuntime`, `switchRuntime` at `:524`).
    Switching a stream session to terminal changes the runtime and confirms first. The
    «Probar modo chat (experimental)» button (`:827-831`) and the «experimental: modo chat»
    pill (`:603`) become a «beta» tag in the Chat segment.
  - Turning Autopiloto on opens a confirmation popover: title «Activar autopiloto», text: the
    chain runs on its own until it opens the PRs and uses quota; Ahorro stays on while it runs;
    buttons Cancelar · Activar. While on, Ahorro is forced on (disabled, tooltip says why).
    Paused: «Autopiloto en pausa · Reanudar», with the reason («límite de 5 h · se reanuda a las
    18:40»).
  - Removes: the separate «Desactivar» buttons (`:605-608`, `:614`), `saving-on` reused for
    «Activar autopiloto» / «Activar modo ahorro» (`:622`, `:627`), `.saving-pill`.
  - Narrow (960): phase shows bars only (label in tooltip), toggles icon + switch.
- **Under the header**, kept and restyled: the `SessionChain` row (handoffs / saving-mode
  steps, «Plan → Implementación → Revisión» with the current one strong) and the base-stale
  notice as a thin neutral bar («main avanzó 3 commits · Actualizar rama», today the `notices`
  bars at `:657-668`).
- **Tabs**: Tarea · sessions · file tabs (CodeMirror editors, closable, mono names) · Cambios
  (closable, count in a `Count` pill) · PR, then a dashed «+ Sesión» button with a menu (Claude
  ⌘⇧T / Terminal ⌘T, the existing `new-claude` / `new-terminal` shortcuts). It replaces «+
  Claude» / «+ Terminal» inside the Tarea tab, which got pushed off screen when SpecKit created
  many tasks.
- **Project variant**: the same screen with owner `@<project>` (`App.tsx:257-258`) shows a
  «Proyecto» tab instead of «Tarea», rendering the project view (`ProjectView` at `:734`).
- **Tarea tab** (`TaskView.tsx`): no stepper (the header has the phase). Meta line (proyecto ·
  flujo · rama mono) with «Marcar como hecha» (secondary, one click) and a «Más» menu (Abrir
  cambios, Archivar workspace, Eliminar tarea…). The pending ask is answered **inline** with the
  shared AskCard; never send the user to the Bandeja to answer. Then Sesiones (rows: dot, name,
  kind, Abrir), **Tareas de la spec** (collapsible by phase, progress, «Abrir tasks.md»),
  Historial (relative times, human sentences).
- **Chat-mode session tab** (`ConversationView.tsx`): conversation (turns, tool calls collapsed
  to one line), live permission cards in the conversation (and an expired variant), composer
  with Enviar / Detener.
- **Terminal tabs**: xterm themed from tokens, following the theme live.
- **Cambios** (`ChangesView.tsx`): file list grouped by folder with a filter, skeleton while
  loading, tree panel auto-hidden on this tab (three panes squeezed the diff), «Recargar», and
  the tree refreshed after commits.
- **PR** (`PrView.tsx`): one friendly error with an action (e.g. no `gh` session: `gh auth
  login`, «Abrir una terminal», «Recargar»), never the raw stderr twice; «Abrir PR» disabled
  with the reason. Open PR: checks (running ones neutral), reviews, «Abrir en GitHub», merge
  disabled until checks pass.
- **Setup failed** (`SetupBanner.tsx`, board TareaSetup): exit code, the command and the output
  tail; «Reintentar setup», «Iniciar sin setup», «Abrir el log completo», link to the project's
  setup command. The header shows «Sin iniciar», no phase bars.
- **Done task** (board TareaHecha): badge Hecha, merged PR chip, relative date; no bars.
- **Agent crashed** (board TareaCaida) and **free workflow with several Claude sessions**
  (board TareaLibre): follow those boards when they land.
- At 1280: rail sidebar, tree hidden, compact header.

### Backlog task (`BacklogTaskView.tsx`)
Same form language as Nueva tarea; actions «Iniciar» (primary), «Guardar», «Eliminar…»
(danger, far right). Shows «Creada ayer» (see §6). After «Iniciar» it shows the started state
(badge Trabajando, toast «LIBRE-9 iniciada · Abrir»).

### Nueva tarea (`NewTaskDialog.tsx`)
- Fixed-height dialog (600×700) with a scrolling body and a sticky footer (today the height
  jumps 303 ↔ 670 px and the start button falls below the fold).
- Origin (radio group): Ticket de Jira | Tarea libre | Con Claude. The Con Claude proposal step
  follows the Conversacion board.
- Dónde trabaja: «En tu checkout» (default, labelled «por defecto») / «Aislado (worktree)»; the
  project's own setting wins when set. See §6b.
- Autopiloto / Ahorro use the same Toggle as the header (same confirmation rule).
- **Vista and chat mode.** Today the dialog has a «Vista» `<select>` (`:363-370`) and a
  «Modo chat (experimental)» checkbox (`:371-376`). They become one «Chat | Terminal»
  segmented under «Opciones avanzadas», Chat tagged «beta»; the default stays as today (`view`
  defaults to `chat`, `:61`).
- Rama and Primer mensaje under «Opciones avanzadas».
- Footer: Cancelar · Guardar en backlog · **Iniciar** (replaces «Lanzar» / «Empezar», `:281`,
  `:477`).

### Jira (`JiraView.tsx`)
- Filters as labelled dropdown chips (De quién, Estado, Sprint) instead of three segmented
  pairs; Agrupar por Estado | Épica.
- Rows on two lines (key + full title / epic · sprint · subtareas) so meta pills stop cutting
  titles. Avatar column only when «De quién» ≠ «Asignados a mí». Priority shown only for
  Alta/Crítica (icon + word).
- Detail: «Abrir en Jira ↗»; if the ticket already has a Delilah task, one clickable row (icon,
  phase · session, status, «Abrir ›»); otherwise primary «Analizar con Claude» with one line on
  what it does. Then fields and description; subtasks with progress and grouped by status.
- Empty: «Ningún ticket con estos filtros» + «Quitar filtros». Connect form: Cloud / Server /
  Datos de ejemplo.
- At 1024: detail as a drawer, filters collapse to «Filtros · N».

### Proyectos (`ProjectsView.tsx`, `ProjectSettings.tsx`)
- Table columns: name (with an icon by kind and «Repo · 4 tareas activas» under it; no «Tipo»
  column), Dónde trabaja, Carpeta (mono), actions.
- «Añadir proyecto» opens a dialog (today the add form is always open under the table).
- Row actions: «Ajustes» + ⋯ (Renombrar, Quitar…); Quitar confirms in a dialog (today: inline,
  `:217-220`). Rows open the project view.
- Project settings is a real dialog (`role="dialog"`), every field with help text.

### Project view (`ProjectView.tsx`)
Reached today as the «Proyecto» tab of the project screen (owner `@name`, see Task screen), not
as a top-level view. Header: folder icon + name + path once; «Preguntar a Claude» (primary,
read-only session in the checkout), «Nueva tarea», «Terminal»; sections Conversaciones / Ideas,
Tareas activas, Comando de preview (with help and Guardar), README preview, file tree.

### Ajustes (`SettingsView.tsx`)
Section index on the left (General, Preguntas, Modo ahorro, Actualizaciones, Atajos de teclado,
Diagnóstico), content max 720 px (today rows span >1000 px). «Por defecto» only on changed
shortcuts; «Restaurar todos». Modo ahorro is described with the decision 33 wording.
Diagnóstico: Logs (opens it), daemon version, data folder.

### Dialogs and overlays
- Delete task (board Eliminar): one ConfirmDialog style for every destructive action.
- Quick Open: files only, as today; matched characters bold; key hints.
- Logs (`LogsView.tsx`): level filter «Todo · Info · Advertencias · Errores», search, «Seguir»,
  «Copiar»; reachable from the statusbar and Ajustes › Diagnóstico (in the browser too, not only
  the native menu).
- Global notices (board AvisosGlobales): toasts, update banner (`UpdateNotice`), quota,
  autopilot pause reasons.

### Widths
- Minimum window width 1024 (already in `tauri.conf.json`). Below ~1300 px: sidebar rail and
  tree auto-hidden. Below ~1200 px: inbox and Jira details open as overlays.

## 6. Needs new daemon / API data

Checked against `app/src/api.ts` and `crates/delilah-daemon`. Re-check each one before
building it; the list is the starting point, not a contract.

**Rules for every change here:** both sides in the same PR (route + handler in
`crates/delilah-daemon/src/*.rs`, registered in `api.rs`; type + call in `app/src/api.ts`;
`crates/delilah-cli` if it uses it), serde shapes and TS types matching. Schema changes are a
new additive `crates/delilah-daemon/migrations/NNNN_*.sql`; never edit a shipped one. Status
changes go through `AppState::transition`; emit the right `Topic` after any change a client
must see. Each behaviour gets a test.

**New endpoints or fields**

| Area | What the design needs | What exists today |
|---|---|---|
| Header / sessions | Chat \| Terminal **per session** (a canvas decision, not one in decisions.md) | `setRuntime(key, mode)` (`POST /tasks/{key}/runtime`, `runtime_api.rs`) is per task; `Session.runtime` is read-only |
| Chat view | Resolved permission cards stay visible with their outcome («Caducada · se respondió en la terminal») | `CardBase.outcome` already has `answered`/`terminal`/`withdrawn` (`api.ts:422`), but `PermissionCardEntry` is dropped from the conversation once answered (`api.ts:436`); keep it, with the command summary |
| Tarea tab | Undo «Marcar como hecha» (Deshacer toast) | `api.done` exists; no reopen endpoint for a task (`reopenSession` is per session) |
| PR tab | Check durations / running time | `PrCheck` has name, outcome, url only; needs started/finished |
| PR tab | Review comments (author, file, text, time) | `PrSnapshot` has `comments` (count) and `review_decision` |
| PR tab | «Fusionar» | no merge endpoint |
| Setup failed | Exit code, duration, full log («Abrir el log completo») | `setup-failed` carries only `reason` (output tail); the `setup` session scrollback may cover the log |
| Historial | Events for runtime switches and expired cards | not checked whether `EventRecord` emits them |
| Backlog task | Created date («Creada ayer») | `tasks.created_at` is in the DB (migration 0001) but not in the API `Task`: expose it, **no migration** |
| Bandeja | «Limpiar avisos» in bulk + restore (Deshacer) | only `dismissNotice` (PR notices) and `dismissReviewHint` |
| Bandeja (empty) | «Última actividad» line | no source; latest task event would do |
| Primer uso | Remember «Omitir Jira» and when to stop showing the checklist | needs a persisted setting |
| Jira | Subtasks of a parent (20 children, any assignee/status) + count/progress on the parent | `jiraIssues(who, progress, sprint)` returns only filtered issues; `JiraIssue` has `parent`/`subtask` but no count. Something like `GET /api/v1/jira/issues/{key}/subtasks` + `subtask_count`/done. Group by `status` name (En revisión lives inside `indeterminate`) |
| Project view | Conversation rows with last message + time | `Session` has no last-message/last-activity (the table has only `created_at`); today it means one `conversation()` per session |
| Ajustes | «Buscar actualizaciones automáticamente» | no such preference; the app always checks |
| Ajustes › Diagnóstico | Data folder + «Mostrar en Finder» | `/health` returns only `version`; needs the home path and a Tauri reveal command (desktop only) |
| Nueva tarea / projects | Checkout as the default workspace | behaviour change, see §6b |

**Already possible with today's data (client only)**
- Board: project filter, PR summary, autopilot marker, spec done/total.
- Jira: linked-task row (task key = ticket key; phase/session from tasks + spec summaries);
  all filters (`assigned`/`reported`/`query`, open/done, all/current); «Filtros · N» chip;
  drawer at 1024.
- Inbox: grouping by task (`InboxItem.task`, `title`); blocking vs non-blocking (`BLOCKING`);
  project from the task.
- Sidebar at scale: «Ver N más», filter, collapsed counts, dots — all from `tasks()`.
- Disconnected state: `health()` + the app's own last-successful-fetch time and retry loop.
- Logs level filter: parse the tracing level from the plain-text `/logs`.
- Nueva tarea: every field exists in `RunRequest` (`in_checkout`, `saving_mode`, `autopilot`
  — feature workflow only —, `notes`, `branch`, `prompt`).
- Proyectos: Renombrar via `updateProject({name})`; «Comando de preview» lives in
  `project.preview_command`, not `ProjectSettings` (saving that dialog = two calls).
- **Project view file tree and README**: `tree` / `files` / `file` take an owner, and the
  daemon resolves `@<project>` to the checkout (`files.rs:161` → `owners::workspace`,
  `PROJECT_OWNER` in `store_sessions.rs:44`). Call them with `@name`.
- Inline AskCard, permission `rule`, `SpecSummary.current`, diff, `TaskPrs.account/problem`,
  `stopSession`, `openSession`, `openProjectSession`, `Autopilot.reason`: all exist.

## 6b. Decisions taken for this refactor

- **Default workspace = the checkout** (work in the user's folder, on a feature branch;
  decision 16). Behaviour change, record it in `docs/decisions.md` next to decision 16:
  - Project setting `settings.workspace = null` now means **checkout**. Today it means
    worktree in three places: the daemon (`run.rs:267-276`, `.unwrap_or(false)`), `whereOf`'s
    `fallback = "worktree"` (`WherePicker.tsx:11-12`) and `BacklogTaskView.tsx:87`
    (`w ?? "worktree"`). `NewTaskDialog.tsx:347` already falls back to checkout.
  - `WherePicker.tsx:30` «Por defecto (aislada)» becomes «Por defecto (en tu checkout)».
  - Existing tasks keep their saved `in_checkout`; only unset ones change. Update the tests
    that assert the old default (`NewTaskDialog.project-settings.test.tsx`,
    `ProjectSettings.test.tsx`, `BacklogTaskView.test.tsx`, daemon tests in `run.rs`).
- **Quitar proyecto.** The daemon refuses with 409 (`projects.rs:265` `delete_project`) while any
  task has a workspace; the design shows a blocked note and a disabled «Quitar» in that case.
- **Stopping is «Detener»** everywhere (glossary, §3).
- **Logs lines** stay as the daemon prints them (English tracing text): data, not UI copy.

## 7. Future (not in this refactor)

- Colour-blind mode: add shapes to status colours and a setting to turn it on.

## 8. Suggested order and acceptance

Each step is shippable, gets its own branch, PR and version bump (§0), and is checked in a
preview in **both themes at 1440×900 and 1280** against the boards listed. Every step also
passes: no raw hex outside the token blocks; no `style={{` in the files it touched except
dynamic values; every visible control works or is disabled with a reason in `title`; no
English in the Spanish UI and no missing en.ts key; tsc, vitest and `scripts/e2e.sh ui` green.

1. **Tokens + base components** (Button, StatusBadge, InboxKind, PrChip, PhaseBars, Toggle,
   Segmented, OptionCard, Row, Count, Dialog, Menu, Skeleton), themed shadows, scrollbars,
   checkboxes, live xterm and CodeMirror themes. Visual-only, no behaviour change.
   Boards: Sistema v2 › Sistema. Done when: no old token name (`--acc`, `--elev`, `--done`,
   `--term`, …) is left; terminal and editor switch theme without reload.
2. **Shell + sidebar + statusbar** (+ disconnected state, Logs button, rail with ⌘\\).
   Boards: Sistema v2 › Lateral, Diálogos y estados › Desconectado, Logs. Done when: stopping
   the preview daemon shows the disconnected state and it recovers alone.
3. **Task screen** (header with title/Preview/toggles/Chat|Terminal, SessionChain and base-stale
   rows, tabs + «+ Sesión», project variant, Tarea tab, inline AskCard, chat tab, Cambios, PR,
   setup failed, done). Boards: Cabecera de tarea › Main, Estados (Antes = today, for
   reference); Pantallas › Tarea; Diálogos y estados › TareaSetup; TareaHecha, TareaCaida,
   TareaLibre when ready. Done when: a pending ask is answered from the Tarea tab without
   opening the Bandeja.
4. **Bandeja** (kinds, Avisos, Limpiar), **board** (+ first run), **Jira**. Boards: Pantallas ›
   Bandeja, Tablero, Jira; Diálogos y estados › PrimerUso. Done when: «Limpiar avisos» never
   removes a blocking item and Deshacer restores the rest.
5. **Nueva tarea, backlog task, Proyectos, project view, Ajustes, dialogs**, and the checkout
   default (§6b). Boards: Diálogos y estados › NuevaTarea, Eliminar, QuickOpen; Pantallas ›
   BacklogTarea, Proyectos, ProyectoVista, Ajustes; Conversacion. Done when: the dialog height
   never changes and «Iniciar» is always visible.
6. **Widths** (rail, overlays) and **scale** (sidebar limits, inbox grouping), plus loading and
   global notices. Boards: Anchos y escala › Tarea1280, Tablero1280, Jira1024, EscalaLateral,
   EscalaJira; Cargando; AvisosGlobales. Done when: nothing scrolls horizontally at 1024 except
   the board columns.

**Tests that will break.** Rename components together with their tests, and move e2e selectors
from class names to roles and text.
- UI e2e (`app/e2e/*.spec.ts`, English UI) uses class selectors `.board .kcard`,
  `.kcard-status`, `.pr-chip`, `.pr-meta`, `.pr-view .pr-card`, `.pr-checks li`,
  `.pr-ci-failure`, `.pgroup .phead`, `.inbox-item`, `.perm`, `.perm-rule`, `.view-switch`,
  `.view-switch-wrap`, `.conv`, `.tool-card`, and labels «Start», «New task» (dialog), «Free
  task» (radio), «Project» (combobox), «Chat» / «Terminal» (buttons).
- Vitest files that assert old labels or classes: `Sidebar`, `TaskScreen`, `TaskView`,
  `AutopilotMark`, `PhaseBadge`, `PrBadge`, `PrView`, `PreviewControl`, `NewTaskDialog` (two
  files), `BacklogTaskView`, `ProjectSettings`, `AskCard`, `PermissionCard`, `SetupBanner`,
  `DeleteTaskDialog`, `ProposalCard`, `ToolCard`, `SessionChain`, `OpenPrDialog`, `DiffRows`,
  `TreeResizer`, `ConversationView` (`app/src/components/*.test.tsx`).

## 9. Before / after record (required at the end)

The UI before the redesign is tagged locally as `ui-v1` (commit 793e4cf) and captured in
`docs/redesign/antes/` by a Playwright script (see `docs/redesign/README.md` for the script
and the exact command). **When the last step lands, run the same script with
`DELILAH_SCREENS=despues`** to fill `docs/redesign/despues/` with the same screens, data,
sizes and themes, and commit the images with that step. If a selector in the script broke
because of the redesign, fix the selector (prefer roles and text), never the framing or the
data, so the pairs stay comparable. Then tell the owner, so the canvas gets an «Antes y
después» page with each pair side by side.

- Script: `app/e2e/screens.spec.ts` (skipped unless `DELILAH_SCREENS` is set). Run alone:
  `cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e && cd app &&
  DELILAH_SCREENS=despues npx playwright test screens` (not `scripts/e2e.sh ui`, which can't
  filter). It refuses to run on a daemon that already has projects/tasks (keys depend on order).
- 46 images (23 screens × dark/light), names match the canvas boards.
- Selectors are roles and Spanish text, and several texts change with this redesign's glossary:
  «Empezar» → «Iniciar», «Ver cambios» → «Abrir cambios», the sidebar «Tarea» button and
  «Backlog (2)» toggle, the «Principal|Main» tab name («Claude principal»), and Logs is opened
  through the `delilah:show-logs` window event (the redesign adds a statusbar button: use it).
  Delete is reached through the sidebar row's context menu. Update those selectors in the step
  that renames them, so the script keeps passing throughout.
- Seed caveats found while capturing: with a pending permission card the daemon keeps the task
  **running**, not waiting-for-user, so LIBRE-5 shows «En ejecución» (decide whether a pending
  permission should make the task «Te espera» — the design assumes it does); the e2e fakes
  have no path to plain `failed`; relative times and temp paths differ between runs.

## Appendix: BASE CSS

Verbatim from the canvas. Mapping to the app: `.dl { --tokens… }` is the dark `:root` block;
`.dl.light { … }` goes inside `@media (prefers-color-scheme: light) { :root { … } }`; drop the
`.dl` prefix from the other rules (or scope them under `body`/`#root`) and rename classes to the
components of §2 as you see fit. Keep `color-scheme: dark` / `light` in each block.

**The `:where(.dl)` trick.** The reset `:where(.dl) :where(button,input,textarea,select)
{font:inherit;color:inherit}` and `:where(.dl) a {…}` have **zero specificity**, so any
component class (`.b-p { color: var(--primary-tx) }`) beats them. A plain
`.dl button { color: inherit }` (0,1,1) out-ranked `.b-p` (0,1,0) and made the primary-button
text invisible (light text on the light-grey button). In the app, write the reset as
`:where(button, input, textarea, select) { font: inherit; color: inherit; }` and keep component
rules as single classes.

```css
body{margin:0}
.dl{--bg:#202125;--side:#1c1d20;--panel:#26272c;--card:#2c2d33;--dialog:#2a2b30;--hov:#33353b;--sel:#3a3c43;--bd:#33353b;--bd2:#41434a;--tx:#d4d5d9;--tx-strong:#e8e9ec;--tx2:#a6a8ae;--tx3:#979aa1;--primary:#d4d5d9;--primary-hov:#e8e9ec;--primary-tx:#1c1d20;--wait:#e8b45c;--wait-bg:#463a22;--wait-tx:#f0c987;--run:#7aa7f0;--run-bg:#2a3550;--run-tx:#a9c6f7;--pause:#ab9ff5;--pause-bg:#36324f;--pause-tx:#c7befa;--ok:#74c287;--ok-bg:#26402f;--ok-tx:#a3dcb1;--err:#ef8a83;--err-bg:#4a2b2a;--err-tx:#f5aaa4;--neutral:#8d8f96;--neutral-bg:#33353b;--neutral-tx:#c2c4c9;--focus:#7aa7f0;--add-bg:#263a2c;--add-tx:#c9e6d0;--del-bg:#43292a;--del-tx:#f2c9c5;--hunk-bg:#2a3242;--term-bg:#26272c;--term-tx:#c9cad0;--shadow:0 16px 40px rgba(0,0,0,.4);--scrim:rgba(16,17,20,.55);
background:var(--bg);color:var(--tx);font-family:-apple-system,BlinkMacSystemFont,'SF Pro Text',system-ui,sans-serif;font-size:13.5px;box-sizing:border-box}
.dl.light{--bg:#f6f6f4;--side:#efefec;--panel:#ffffff;--card:#ffffff;--dialog:#ffffff;--hov:#ebebe8;--sel:#e3e4e8;--bd:#e3e3df;--bd2:#d2d2cd;--tx:#26272b;--tx-strong:#111214;--tx2:#55585e;--tx3:#6b6e75;--primary:#26272b;--primary-hov:#111214;--primary-tx:#ffffff;--wait:#c58a2a;--wait-bg:#fbf0dc;--wait-tx:#7a4d0b;--run:#2f6fd0;--run-bg:#e5eefc;--run-tx:#1d4f9c;--pause:#6a55c9;--pause-bg:#eeeafb;--pause-tx:#4a3a9c;--ok:#2f8a46;--ok-bg:#e3f3e7;--ok-tx:#1f6234;--err:#c9433b;--err-bg:#fbe6e4;--err-tx:#9a2a23;--neutral:#8a8d93;--neutral-bg:#ececea;--neutral-tx:#4a4d53;--focus:#2f6fd0;--add-bg:#e3f3e7;--add-tx:#1f6234;--del-bg:#fbe6e4;--del-tx:#9a2a23;--hunk-bg:#e8eef9;--term-bg:#ffffff;--term-tx:#26272b;--shadow:0 16px 40px rgba(20,20,30,.14);--scrim:rgba(30,30,35,.42)}
.dl *{box-sizing:border-box}
:where(.dl) :where(button,input,textarea,select){font:inherit;color:inherit}
.dl button{cursor:pointer}
.dl :focus-visible{outline:2px solid var(--focus);outline-offset:2px}
:where(.dl) a{color:var(--run-tx)}
.mono{font-family:ui-monospace,'SF Mono',Menlo,monospace}
.t-view{margin:0;font-size:19px;font-weight:600;color:var(--tx-strong)}
.t-sec{margin:0;font-size:15px;font-weight:600;color:var(--tx-strong)}
.t-label{margin:0;font-size:12px;font-weight:600;color:var(--tx2)}
.muted{color:var(--tx2)}.faint{color:var(--tx3)}.small{font-size:12px}
.b{display:inline-flex;align-items:center;justify-content:center;gap:7px;height:32px;padding:0 14px;border-radius:7px;font-size:13.5px;font-weight:500;white-space:nowrap;text-decoration:none;border:1px solid transparent}
.b-p{background:var(--primary);color:var(--primary-tx)}
.b-s{background:var(--card);border-color:var(--bd2);color:var(--tx)}
.b-g{background:transparent;color:var(--tx2)}
.b-d{background:transparent;border-color:var(--err);color:var(--err)}
.b.sm{height:28px;padding:0 10px;font-size:12.5px}
.b:disabled{opacity:.5;cursor:not-allowed}
.ib{width:32px;height:32px;display:inline-flex;align-items:center;justify-content:center;border-radius:7px;border:0;background:transparent;color:var(--tx2)}
.badge{display:inline-flex;align-items:center;gap:6px;height:22px;padding:0 8px;border-radius:11px;font-size:12px;font-weight:500;white-space:nowrap;background:var(--neutral-bg);color:var(--neutral-tx)}
.badge::before{content:"";width:6px;height:6px;border-radius:3px;background:var(--neutral)}
.badge.run{background:var(--run-bg);color:var(--run-tx)}.badge.run::before{background:var(--run)}
.badge.wait{background:var(--wait-bg);color:var(--wait-tx)}.badge.wait::before{background:var(--wait)}
.badge.pause{background:var(--pause-bg);color:var(--pause-tx)}.badge.pause::before{background:var(--pause)}
.badge.ok{background:var(--ok-bg);color:var(--ok-tx)}.badge.ok::before{background:var(--ok)}
.badge.err{background:var(--err-bg);color:var(--err-tx)}.badge.err::before{background:var(--err)}
.kind{display:inline-flex;align-items:center;gap:5px;font-size:12px;font-weight:600;color:var(--tx2)}
.prchip{display:inline-flex;align-items:center;gap:6px;height:22px;padding:0 8px;border-radius:6px;border:1px solid var(--bd2);font-size:12px;color:var(--tx)}
.dot{display:inline-block;width:7px;height:7px;border-radius:4px;flex:none}
.seg{display:inline-flex;padding:2px;gap:2px;border-radius:8px;background:var(--panel);border:1px solid var(--bd2)}
.seg button{height:26px;padding:0 10px;border:0;border-radius:6px;background:transparent;color:var(--tx2);display:inline-flex;align-items:center;gap:6px}
.seg button[aria-pressed="true"],.seg button[aria-selected="true"]{background:var(--sel);color:var(--tx-strong)}
.toggle{display:inline-flex;align-items:center;gap:7px;height:30px;padding:0 11px;border-radius:8px;border:1px solid var(--bd2);background:transparent;color:var(--tx2);font-size:13px}
.toggle .sw{width:22px;height:13px;border-radius:7px;background:var(--bd2);position:relative;flex:none}
.toggle .sw::after{content:"";position:absolute;top:2px;left:2px;width:9px;height:9px;border-radius:5px;background:var(--tx2)}
.toggle[aria-pressed="true"]{background:var(--sel);color:var(--tx-strong);border-color:var(--sel)}
.toggle[aria-pressed="true"] .sw{background:var(--tx-strong)}
.toggle[aria-pressed="true"] .sw::after{left:11px;background:var(--sel)}
.opt{display:flex;flex-direction:column;gap:4px;padding:10px 12px;border-radius:9px;border:1px solid var(--bd2);background:var(--panel);color:var(--tx);text-align:left;position:relative}
.opt[aria-pressed="true"]{background:var(--sel);border-color:var(--tx3)}
.opt[aria-pressed="true"]::after{content:"✓";position:absolute;top:8px;right:10px;font-size:12px;color:var(--tx-strong)}
.row{display:flex;align-items:center;gap:10px;min-height:32px;padding:0 10px;border-radius:7px;border:0;background:transparent;color:var(--tx);text-align:left;text-decoration:none;width:100%}
.row[aria-current="true"],.row[aria-selected="true"]{background:var(--sel);color:var(--tx-strong)}
.tabs{display:flex;align-items:flex-end;gap:2px;height:40px;padding:0 18px;border-bottom:1px solid var(--bd)}
.tab{display:inline-flex;align-items:center;gap:7px;height:36px;padding:0 14px;border:0;border-bottom:2px solid transparent;background:transparent;color:var(--tx2)}
.tab[aria-selected="true"]{border-bottom-color:var(--tx-strong);color:var(--tx-strong)}
.card{border-radius:10px;background:var(--panel);border:1px solid var(--bd)}
.in{height:34px;padding:0 10px;border-radius:7px;border:1px solid var(--bd2);background:var(--panel);color:var(--tx);width:100%}
textarea.in{height:auto;padding:10px;resize:none;line-height:1.5}
.field{display:flex;flex-direction:column;gap:6px;min-width:0}
.field label,.field .lab{font-size:12px;color:var(--tx2)}
.help{font-size:12px;color:var(--tx3);line-height:1.45}
.kbd{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:var(--tx2);border:1px solid var(--bd2);border-radius:4px;padding:1px 5px}
.cnt{min-width:20px;height:18px;padding:0 6px;border-radius:9px;font-size:11px;font-weight:600;display:inline-flex;align-items:center;justify-content:center;background:var(--neutral-bg);color:var(--tx2)}
.cnt.hot{background:var(--wait);color:#2a1a05}
.sep{flex:none;width:1px;height:20px;background:var(--bd2)}
.sk{display:block;height:12px;border-radius:6px;background:var(--hov)}
.menu{padding:6px;border-radius:10px;background:var(--dialog);border:1px solid var(--bd2);box-shadow:var(--shadow);display:flex;flex-direction:column;gap:2px}
.menu button,.menu a{display:flex;align-items:center;gap:10px;min-height:32px;padding:0 10px;border:0;border-radius:6px;background:transparent;color:var(--tx);text-align:left;text-decoration:none}
.dialog{border-radius:14px;background:var(--dialog);border:1px solid var(--bd2);box-shadow:var(--shadow);display:flex;flex-direction:column}
.dialog-ft{display:flex;align-items:center;justify-content:flex-end;gap:8px;padding:14px 22px;border-top:1px solid var(--bd)}
.bars{display:inline-flex;gap:3px}.bars i{display:block;width:14px;height:4px;border-radius:2px;background:var(--bd2)}.bars i.on{background:var(--tx-strong)}.bars i.ok{background:var(--ok)}
.bars.mini i{width:10px;height:3px}
.dl ::-webkit-scrollbar{width:10px;height:10px}
.dl ::-webkit-scrollbar-track{background:transparent}
.dl ::-webkit-scrollbar-thumb{background:var(--bd2);border-radius:5px;border:2px solid transparent;background-clip:padding-box}
.dl ::-webkit-scrollbar-thumb:hover{background:var(--tx3);background-clip:padding-box;border:2px solid transparent}
.dl{scrollbar-color:var(--bd2) transparent}
.dl input[type="checkbox"],.dl input[type="radio"]{accent-color:var(--tx-strong);width:15px;height:15px}
```
