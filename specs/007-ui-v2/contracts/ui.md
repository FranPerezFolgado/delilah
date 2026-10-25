# Contract: UI kit, roles, selectors and copy (UI v2)

What other code (screens, vitest, Playwright e2e, the screenshot script) may rely on.
Visual details (sizes, colours, spacing) are in `docs/ui-redesign.md` §1, §2 and the
appendix; this file fixes names, props, roles and the strings tests look for.

## Conventions

- Kit in `app/src/components/ui/` (re-exported by `ui/index.ts`); each component has
  `X.tsx`, `X.css`, `X.test.tsx`. Screens get `<Screen>.css` next to them.
- `styles.css` holds only tokens (dark `:root`, light `@media (prefers-color-scheme:
  light)`), the `:where(button,input,textarea,select)` reset, focus, scrollbars,
  checkboxes/radios, `.mono .t-view .t-sec .t-label .muted .faint .small` and the shell grid.
- No raw hex outside the two token blocks (the `.cnt.hot` text colour becomes a token,
  `--wait-on`: dark `#2a1a05`, light `#2a1a05`).
- **Scale (enforced by `styles.test.ts`)**: `font-size` only 19 / 15 / 13.5 / 12 px (mono
  12); `border-radius` only 7 (controls) / 10 (cards) / 14 (dialogs) / 11 (pills). This
  **supersedes the appendix** on two values: `.b.sm` font-size 12 (not 12.5) and `.prchip`
  radius 7 (not 6); `.toggle` font-size 13.5 (not 13). Allow-list of sub-element values kept
  in the test, each with a comment: `.kbd` 11 px / radius 4, `.cnt` 11 px / radius 9,
  `.bars i` radius 2, `.dot` radius 4, `.toggle` radius 8 and its `.sw` 7 / knob 5, `.seg`
  radius 8 and its buttons 6, `.menu` items radius 6, `.opt` radius 9, `.sk` radius 6.
- **Contrast (enforced by `tokens.test.ts`)**: ≥ 4.5:1 for the pairs of brief §1 «Contrast»
  in both themes. No `style={{` except the allow-list in
  `app/src/inline-style.test.ts` (dynamic values such as `--tree-w`).
- English for new copy goes in `app/src/i18n/en/<Component>.ts` (`export const EN`).
- Icons: `Icon.tsx`, stroke 1.5, viewBox 16, `aria-hidden` when decorative; no emoji.

## Kit components (props are the contract; extra optional props allowed)

Only components with a caller in this PR are built; each kit task names its first caller,
and one without a caller is dropped (no speculative kit).

| Component | Props | Renders / roles |
|---|---|---|
| `Button` | `variant: "primary"\|"secondary"\|"ghost"\|"danger"`, `size?: 32\|28`, `icon?`, native button props | `<button class="b b-p\|b-s\|b-g\|b-d [sm]">`; `danger` always has a red border; disabled needs `title` (reason) |
| `IconButton` | `label` (required), `icon`, `pressed?` | `<button aria-label>`; `aria-pressed` when `pressed` given |
| `StatusBadge` | `status: TaskStatus \| "backlog"`, `detail?: boolean` | `<span class="badge <tone>">` text from `vocab.statusOf`; `title` = detail/reason |
| `InboxKind` | `kind: InboxKind` | icon + «Pregunta» / «Permiso» / «Aviso», no background |
| `PrChip` | `pr: PullRequest` (or `{number, state}`) | PR icon + `#n` + «Borrador»\|«Abierta»\|«Checks fallan»\|«Fusionada» |
| `PhaseBars` | `phase: HeaderPhase \| null`, `status: TaskStatus`, `mini?`, `labelInTooltip?` | 4 segments + «Plan · 2/4» (+ «· 6/14 tareas»); `done` → nothing; `setup-failed` → «Sin iniciar» |
| `Toggle` | `label`, `pressed`, `onChange`, `disabled?`, `title?`, `iconOnly?` | `<button class="toggle" aria-pressed>` with the switch |
| `Segmented` | `mode: "radio"\|"pressed"`, `label`, `options: {value,label,tag?,disabled?,title?}[]`, `value`, `onChange` | radio: `role="radiogroup"` + `role="radio"` + `aria-checked`; pressed: buttons with `aria-pressed`; same look; `tag` renders a «beta» chip |
| `OptionCard` | `checked`, `title`, `description?`, `name` (group) | `role="radio"` + `aria-checked` inside a `radiogroup` (✓ when checked; CSS matches `[aria-checked="true"]` as well as the canvas's `[aria-pressed]`) |
| `Row` | `as?: "button"\|"a"`, `current?`, `selected?`, `leading?`, `trailing?`, `title?` | `.row` with `aria-current="true"` / `aria-selected`; one height (32) and a two-line variant (52) |
| `Count` | `n`, `hot?` | `.cnt[.hot]`; hidden when `n = 0` unless `showZero` |
| `Skeleton` | `lines?`, `width?` | `.sk` blocks with `aria-busy` on the container |
| `Dialog` | `title`, `onClose`, `footer?`, `size?: {w,h}` | `role="dialog" aria-modal="true" aria-labelledby`, focus trap, Esc, returns focus to the opener, scrim `--scrim` |
| `ConfirmDialog` | `title`, `body`, `action` (label), `danger?`, `onConfirm`, `blocked?: string` | footer **Cancelar · Acción**; `blocked` shows the note and disables the action |
| `Menu` | `trigger` (render), `items: {label, onSelect, danger?, disabled?, title?, shortcut?}[]` | `role="menu"`/`menuitem`, ↑↓, Home/End, Esc, focus returns |
| `Toast` / `ToastHost` / `useToast()` | `toast({text, action?: {label, run}, ttl?})` | `role="status"`; one stack, bottom-left of main; `ttl` default 6 s |

Board card autopilot marker: a small autopilot icon (`Icon.tsx`) with its state in the
tooltip, not `AutopilotControls`.

Domain components (in `components/`): `AskCard` (props `item` | `entry`, `onAnswer`,
`variant: "inbox"|"inline"|"chat"`), `TaskHeader`, `TaskTabs`, `SessionViewSwitch`,
`AutopilotControls`, `DisconnectedBanner`, `FirstRun`, `AddProjectDialog`,
`GlobalNotices`. Removed at the end: `PhaseBadge`, `PrBadge`, `AutopilotMark`,
`components/status.ts`, `.btn*`, `.pill`, `.segs/.seg` (old), `.nav`, `.srow`,
`.phead-main`, `.bhead`, `.inbox-task`, `.kcard-status`, `.tag` (status), `.pcount`,
`.saving-*`, `.phase-pill`, `.stepper`, `.view-switch*`.

## Mappings (`app/src/vocab.ts`)

- `statusOf(status | "backlog") → { tone: "neutral"|"run"|"wait"|"pause"|"ok"|"err", label, detail? }` per brief §3.
- `kindOf(InboxKind) → { kind: "Pregunta"|"Permiso"|"Aviso", blocking: boolean, clearable: boolean }`.
- `prStateOf(PullRequest) → "draft"|"open"|"failing"|"merged"` (+ label).
- `sessionDot(Session) → "wait"|"run"|"neutral"`.
- `BLOCKING` equality with the daemon: `vocab.test.ts` reads
  `crates/delilah-daemon/tests/fixtures/blocking_kinds.json`.

## Breakpoints (`app/src/useWidth.ts`)

Window (`useBreakpoints`, `matchMedia`): `rail` < 1300 px (sidebar rail, tree hidden) ·
`overlays` < 1200 px (inbox/Jira detail as overlay/drawer) · minimum window 1024.
Main area (`useContainerWidth(ref)`, `ResizeObserver`): `compact` when the **main area**
(not the window) is ≤ 960 px — task header shows bars only and icon toggles.

## Shortcuts (`app/src/shortcuts.ts`)

New `ActionId` `"toggle-sidebar"`, default `"Cmd+\\"`, group «Ventana», label «Plegar barra
lateral». Existing `new-claude` (⌘⇧T) and `new-terminal` (⌘T) feed «+ Sesión».

## Accessible names and texts tests rely on (Spanish UI / English UI)

| Where | Element | Spanish | English |
|---|---|---|---|
| Statusbar | button | «Logs» | «Logs» |
| Statusbar | buttons | «N trabajando» / «N te espera» / «N en pausa» / «N con error» | «N working» / «N waiting for you» / «N paused» / «N with errors» |
| Statusbar | status | «Conectado» / «Sin conexión con delilahd · reintentando» | «Connected» / «No connection to delilahd · retrying» |
| Banner | button | «Reintentar ahora» | «Retry now» |
| Sidebar | nav links | «Bandeja», «Tareas», «Jira», «Proyectos» | «Inbox», «Tasks», «Jira», «Projects» |
| Sidebar | button | «Añadir proyecto» | «Add project» |
| Sidebar | button | «Backlog · N» (`aria-expanded`) | «Backlog · N» |
| Sidebar | button | «Plegar barra lateral» | «Collapse sidebar» |
| Sidebar | row context menu (`Menu`) | «Abrir la tarea», «Marcar como hecha», «Eliminar tarea…» (→ `ConfirmDialog`) | «Open the task», «Mark as done», «Delete task…» |
| Task header | phase button → `Menu` | the 4 phases, current checked | same |
| Chat | transcript line after Detener | «Detenida: detuviste a Claude.» | «Stopped: you stopped Claude.» |
| PR tab | merge confirm | `ConfirmDialog` «Fusionar #n» (Cancelar · Fusionar); disabled reason «No se pudo saber qué métodos de fusión permite el repositorio» | «Merge #n»; «Couldn't tell which merge methods the repository allows» |
| New task / backlog | dirty-checkout error | «Tu checkout tiene cambios sin guardar…» + «Iniciar en un worktree», «Abrir una terminal» | «Your checkout has uncommitted changes…» + «Start in a worktree», «Open a terminal» |
| Board | button (primary) | «Nueva tarea» | «New task» |
| Board | card | `role="link"` named «<KEY> <title>» | same |
| New task | dialog | «Nueva tarea» | «New task» |
| New task | radios | «Ticket de Jira», «Tarea libre», «Con Claude» | «Jira ticket», «Free task», «With Claude» |
| New task | combobox | «Proyecto» | «Project» |
| New task | buttons | «Cancelar», «Guardar en backlog», «Iniciar» | «Cancel», «Save to backlog», «Start» |
| Task header | pressed buttons | «Chat» (tag «beta»), «Terminal» | «Chat», «Terminal» |
| Task header | toggles | «Autopiloto», «Ahorro» | «Autopilot», «Saving» |
| Task tabs | tablist | «Tarea» / «Proyecto», session labels («Claude principal»), «Cambios», «PR» | «Task» / «Project», «Main Claude», «Changes», «PR» |
| Task tabs | button | «+ Sesión» (menu «Claude», «Terminal») | «+ Session» |
| Tarea tab | buttons | «Marcar como hecha», «Más» (menu «Abrir cambios», «Archivar workspace», «Eliminar tarea…») | «Mark as done», «More», «Open changes», «Archive workspace», «Delete task…» |
| Toast | button | «Deshacer» | «Undo» |
| Ask card | buttons | «Enviar»; «Permitir una vez», «Permitir siempre en esta tarea», «Denegar»; «Quitar» | «Send»; «Allow once», «Always allow in this task», «Deny»; «Remove» |
| Bandeja | button | «Limpiar avisos» | «Clear notices» |
| Bandeja | list | `role="listbox"`, items `role="option"` | same |
| PR tab | buttons | «Abrir PR», «Abrir en GitHub», «Fusionar», «Recargar» | «Open PR», «Open in GitHub», «Merge», «Reload» |
| PR tab | list | checks `role="list"` named «Checks» | «Checks» |
| Setup failed | buttons | «Reintentar setup», «Iniciar sin setup», «Abrir el log completo» | «Retry setup», «Start without setup», «Open the full log» |
| Chat | region | `role="log"` named «Conversación» | «Conversation» |
| Chat | tool call | `role="group"` named by the tool summary | same |
| Chat | permission card | `role="group"` named «Permiso: <tool>» | «Permission: <tool>» |
| Logs | radios | «Todo», «Info», «Advertencias», «Errores» | «All», «Info», «Warnings», «Errors» |

## Copy renames (Spanish key → new Spanish key; old en.ts key deleted at the end)

| Old | New |
|---|---|
| «Lanzar», «Empezar», «Empezar {key}» | «Iniciar», «Iniciar {key}» |
| «Sin empezar» | «Sin iniciar» |
| «Parar» (any) | «Detener» |
| «Ver cambios», «Ver …», «Ir a …» | «Abrir cambios», «Abrir …» |
| «Refrescar» | «Recargar» |
| «Saltar» | «Omitir» |
| «Por defecto (aislada)» | «Por defecto (en tu checkout)» |
| «Principal» (tab) | «Claude principal» |
| «Avisos» (logs filter) | «Advertencias» |
| «workflow» | «flujo» |
| «Inactiva» (session) | (dot only) |
| «Agentes trabajando N» | statusbar counts |
| «experimental: modo chat», «Probar modo chat (experimental)», «Modo chat (experimental)» | «Chat» + «beta» tag |

Untranslated today (brief §3), each fixed by the task that owns the screen: «sample data»,
«Filter tickets…», «Connection», «Assigned/Open/All», «Analyse with Claude» (Jira);
«Search 4 files…» (Quick Open); «REMOTE CONTROL» (Ajustes); «0 archivos» (Cambios/tree).

## E2E selector moves (class → role/text)

| Old selector | New | Specs |
|---|---|---|
| `.board .kcard` | `getByRole("link", { name: /<KEY>/ })` in `region "Tareas"` | board, screens |
| `.kcard-status` | badge text inside the card | board |
| `.pr-chip`, `.pr-meta` | text «#n» + state inside the card / PR tab | board, pr |
| `.pr-view .pr-card` | `getByRole("tabpanel", { name: "PR" })` | pr |
| `.pr-checks li` | `getByRole("list", { name: "Checks" }).getByRole("listitem")` | pr |
| `.pr-ci-failure` | text of the failing check | pr |
| `.pgroup .phead` | `getByRole("button", { name: <project> })` in the sidebar | projects, screens |
| `.inbox-item` | `getByRole("option")` in the inbox listbox | inbox |
| `.perm`, `.perm-rule` | `getByRole("group", { name: /Permission/ })` + rule text | chat |
| `.view-switch`, `.view-switch-wrap` | `getByRole("button", { name: "Chat" \| "Terminal", pressed })` in the header | chat, conversation |
| `.conv` | `getByRole("log", { name: "Conversation" })` | chat, conversation |
| `.tool-card` | `getByRole("group", { name: <summary> })` | conversation |
| sidebar «Tarea» button | board «New task» or ⌘N | screens |
| «Backlog (2)» toggle | «Backlog · 2» | screens |
| `delilah:show-logs` event | statusbar «Logs» button | screens |
