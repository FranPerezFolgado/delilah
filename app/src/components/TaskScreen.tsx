import { useCallback, useEffect, useId, useMemo, useReducer, useRef, useState, type CSSProperties } from "react";
import { EditorView } from "@codemirror/view";
import type { EditorState } from "@codemirror/state";
import { api, ConflictError, type AskPosition, type BaseStatus, type ChangeKind, type FileContent, type InboxItem, type Project, type PullRequest, type Session, type SessionKind, type SpecSummary, type Task, type TaskView as ViewMode } from "../api";
import { AskCard } from "./AskCard";
import { ConversationView } from "./ConversationView";
import { createState, languageFor, languageSlot } from "../editor/codemirror";
import { CHANGES, OFFICE, OVERVIEW, PRS, persist, sessionFor, sessionOf, sessionTab, setTreeOpen, treeOpen, treeWidth, type Tab } from "../editor/session";
import { t } from "../i18n";
import { on } from "../live";
import { isMarkdown } from "../markdown";
import { comboFromEvent, keyOf } from "../shortcuts";
import { ChangesView } from "./ChangesView";
import { TreeResizer } from "./TreeResizer";
import { FileTree } from "./FileTree";
import { Markdown } from "./Markdown";
import { QuickOpen } from "./QuickOpen";
import { Icon, IconButton, useToast } from "./ui";
import { SessionChain } from "./SessionChain";
import { SessionViewSwitch } from "./SessionViewSwitch";
import { TaskHeader } from "./TaskHeader";
import { useBreakpoints } from "../useWidth";
import { OfficeTab } from "../playground/OfficeTab";
import { TaskTabs, type TaskTabItem } from "./TaskTabs";
import { headerPhaseOf } from "../phases";
import { isSetupRunning } from "./sessionLabels";
import { sessionDot } from "../vocab";
import { PrView } from "./PrView";
import { ProjectView } from "./ProjectView";
import { TaskView } from "./TaskView";
import { SetupBanner } from "./SetupBanner";
import { TerminalPane } from "./TerminalPane";
import "./TaskScreen.css";

interface Props {
  /** The app's connection state, for the Oficina tab (the app already holds the change feed). */
  online?: boolean;
  /** The task's key, or `@<project>` for a project's own checkout. */
  owner: string;
  /** The task, when the owner is one. */
  task?: Task;
  /** The task's spec, for the phase shown in the header. */
  spec?: SpecSummary;
  /** The project, when the owner is one. */
  project?: Project;
  /** Opens the new task dialog (from a project). */
  onNewTask?: () => void;
  /** "Convertir en tarea" on one of the project's Claude sessions. */
  onConvert?: (session: Session) => void;
  /** Goes to a task, e.g. the one a conversation created. */
  onOpenTask?: (key: string) => void;
  /** Opens a project's settings (the setup command lives there). */
  onOpenSettings?: (project: string) => void;
  /** All tasks: a project's screen lists its active ones. */
  tasks?: Task[];
  /** The task's open sessions: each one is a tab. */
  sessions: Session[];
  /** Questions with options this task's sessions are waiting on. */
  asks: InboxItem[];
  /** The task's base-stale warnings, shown above the tabs until dismissed. */
  notices: InboxItem[];
  /** Where those questions sit, next to the session's terminal. */
  askPosition: AskPosition;
  onChanged: () => void;
}

const WATCH_MS = 3000;

/** The base name without its remote: `origin/main` -> `main`. */
const shortBase = (base: string | null) => (base ?? "main").replace(/^origin\//, "");

/**
 * The base-stale warnings above the tabs. When the base has moved on (the daemon fetches
 * to say how far) the first one offers to merge it into the branch, and says how it went
 * in a toast.
 */
function BaseStaleBars({ owner, notices, onChanged }: { owner: string; notices: InboxItem[]; onChanged: () => void }) {
  const toast = useToast();
  const [status, setStatus] = useState<BaseStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const has = notices.length > 0;
  useEffect(() => {
    setStatus(null);
    if (!has || owner.startsWith("@")) return;
    let alive = true;
    api
      .baseStatus(owner)
      .then((s) => alive && setStatus(s))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [owner, has]);

  const update = () => {
    setBusy(true);
    api
      .updateBranch(owner)
      .then((r) => {
        const base = shortBase(r.base);
        toast({
          text:
            r.merged === 0
              ? t("La rama ya estaba al día")
              : r.merged === 1
                ? t("Rama actualizada: 1 commit de {base} integrado", { base })
                : t("Rama actualizada: {n} commits de {base} integrados", { n: r.merged, base }),
        });
        setStatus((s) => (s ? { ...s, behind_by: 0 } : s));
        onChanged();
      })
      .catch((e) => toast({ text: (e as Error).message, kind: "error" }))
      .finally(() => setBusy(false));
  };

  const behind = status?.behind_by ?? 0;
  return (
    <>
      {notices.map((n, i) => (
        <div key={n.notice} className="ts-bar base-stale">
          <span className="ts-grow">{n.text}</span>
          {i === 0 && behind > 0 && (
            <>
              <span>
                {behind === 1
                  ? t("{base} avanzó 1 commit", { base: shortBase(status!.base) })
                  : t("{base} avanzó {n} commits", { base: shortBase(status!.base), n: behind })}
              </span>
              <button className="btn btn-ghost" disabled={busy} onClick={update}>
                {t("Actualizar rama")}
              </button>
            </>
          )}
          {n.notice != null && (
            <button className="btn btn-ghost" onClick={() => api.dismissNotice(n.notice!).then(onChanged, () => {})}>
              {t("Descartar")}
            </button>
          )}
        </div>
      ))}
    </>
  );
}

/**
 * A task as Orca shows a worktree: tabs for the task itself, its sessions (Claude and
 * terminals), its changes and the files opened from the project tree on the right. The agent keeps working in the same
 * files, so open files follow the disk, and a save never overwrites what it wrote in
 * the meantime.
 */
export function TaskScreen({ online, owner, task, spec, project, tasks, sessions, asks, notices, askPosition, onChanged, onNewTask, onConvert, onOpenTask, onOpenSettings }: Props) {
  const session = useMemo(() => sessionFor(owner), [owner]);
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);
  const [error, setError] = useState<string | null>(null);
  const [closing, setClosing] = useState<string | null>(null);
  const [changed, setChanged] = useState<Map<string, ChangeKind>>(new Map());
  const [treeRefresh, setTreeRefresh] = useState(0);
  // Below the rail breakpoint the tree starts hidden (the user can still open it).
  const { rail } = useBreakpoints();
  const [showTree, setShowTree] = useState(() => treeOpen() && !rail);
  const railSeen = useRef(rail);
  useEffect(() => {
    if (railSeen.current === rail) return;
    railSeen.current = rail;
    setShowTree(!rail && treeOpen());
  }, [rail]);
  const [treeW, setTreeW] = useState(treeWidth);
  const treeId = useId();
  const [closingSession, setClosingSession] = useState<Session | null>(null);
  const [finding, setFinding] = useState(false);
  // A session just started: its tab shows before the next refresh lists it.
  const [starting, setStarting] = useState<string | null>(null);
  // Terminals stay mounted once shown, so switching tabs keeps their scrollback.
  const [mounted, setMounted] = useState<Set<string>>(() => new Set());
  // Per Claude session, the view the user picked with the Chat / Terminal switch (in the
  // header); without one, the task's setting decides (a project's sessions open in chat).
  // Kept while the screen is open.
  const [views, setViews] = useState<Map<string, ViewMode>>(() => new Map());
  // A task's Claude shows what its runtime is (the switch changes the runtime); a project's
  // sessions have no runtime to choose, so they only change what is shown.
  const viewOf = (s: Session): ViewMode =>
    s.kind === "shell"
      ? "terminal"
      : project
        ? s.runtime === "stream"
          ? "chat"
          : (views.get(s.id) ?? "chat")
        : (s.live ? s.runtime : (s.effective_runtime ?? s.runtime)) === "stream"
          ? "chat"
          : "terminal";
  const showView = (id: string, v: ViewMode) => setViews((cur) => new Map(cur).set(id, v));
  // A project always has its checkout; a task only once its workspace exists.
  const hasWorkspace = !!project || sessions.length > 0;
  // The PR tab: for a task with a workspace (its PR is coming) or with PRs already linked.
  const [prs, setPrs] = useState<PullRequest[]>([]);
  const hasPrs = prs.length > 0;
  const prTab = !project && (hasWorkspace || hasPrs);
  useEffect(() => {
    if (project) return;
    let alive = true;
    const load = () =>
      api
        .taskPrs(owner)
        .then((d) => alive && setPrs(d.prs))
        .catch(() => {});
    api.refreshPrs(owner).catch(() => {});
    load();
    const off = on("prs", load);
    return () => {
      alive = false;
      off();
    };
  }, [owner, project]);

  const tab = (path: string) => session.tabs.find((t) => t.path === path);

  // The tab on screen: the active one if it still exists (a session may have been
  // closed elsewhere, a file may not have loaded yet), else the task's.
  const activeSession = sessionOf(session.active);
  const exists =
    session.active === OVERVIEW ||
    session.active === OFFICE ||
    (session.active === CHANGES && session.changesOpen) ||
    (session.active === PRS && prTab) ||
    !!tab(session.active) ||
    (activeSession !== null && (sessions.some((s) => s.id === activeSession) || activeSession === starting));
  const shown = exists ? session.active : OVERVIEW;
  const shownRef = useRef(shown);
  shownRef.current = shown;
  // An error belongs to the tab it came up in.
  useEffect(() => setError(null), [shown]);
  const active = tab(shown);
  const shownSession = sessionOf(shown);
  const shownClaude = sessions.find((s) => s.id === shownSession && s.kind !== "shell");
  if (shownSession && !mounted.has(shownSession)) setMounted(new Set(mounted).add(shownSession));

  // The live state of a file: the view's for the active one, the stored one otherwise.
  const stateOf = (path: string): EditorState | undefined =>
    path === session.active && view.current ? view.current.state : session.states.get(path);

  const setText = (path: string, f: FileContent) => {
    const t = tab(path);
    if (!t) return;
    t.version = f.version;
    t.size = f.size;
    t.conflict = false;
    t.dirty = false;
    const state = stateOf(path);
    if (f.text === null || !state) {
      t.saved = null;
      return;
    }
    // Replace the text in place: undo history and scroll stay, the cursor is kept in range.
    const head = Math.min(state.selection.main.head, f.text.length);
    const tr = state.update({ changes: { from: 0, to: state.doc.length, insert: f.text }, selection: { anchor: head } });
    // Before dispatching: the change listener compares against it.
    t.saved = tr.state.doc;
    if (path === session.active && view.current) {
      view.current.dispatch(tr);
      view.current.focus();
    } else session.states.set(path, tr.state);
  };

  const save = useCallback(
    async (path: string) => {
      const t = session.tabs.find((x) => x.path === path);
      const state = path === session.active && view.current ? view.current.state : session.states.get(path);
      if (!t || !state || !t.saved) return;
      try {
        const f = await api.saveFile(owner, path, state.doc.toString(), t.version);
        t.version = f.version;
        t.saved = state.doc;
        t.dirty = false;
        t.conflict = false;
        setError(null);
        setTreeRefresh((n) => n + 1);
      } catch (e) {
        if (e instanceof ConflictError) t.conflict = true;
        else setError((e as Error).message);
      }
      redraw();
    },
    [session, owner],
  );
  const saveRef = useRef(save);
  saveRef.current = save;

  const activate = (id: string) => {
    if (tab(session.active) && view.current) session.states.set(session.active, view.current.state);
    session.active = id;
    const state = session.states.get(id);
    if (state && view.current) {
      view.current.setState(state);
      view.current.focus();
    }
    setError(null);
    persist(owner, session);
    redraw();
  };

  const openFile = async (path: string, show = true) => {
    if (!tab(path)) {
      let f: FileContent;
      try {
        f = await api.file(owner, path);
      } catch (e) {
        setError((e as Error).message);
        return;
      }
      // Opened meanwhile by another call (a double click, React's dev double effect).
      if (tab(path)) return show ? activate(path) : undefined;
      // A Markdown file that was shown rendered last time comes back rendered.
      const preview = isMarkdown(path) && session.pendingPreview.delete(path);
      const t: Tab = { path, version: f.version, size: f.size, saved: null, dirty: false, conflict: false, preview };
      if (f.text !== null) {
        const state = createState(f.text, {
          onSave: () => saveRef.current(path),
          onChange: (s) => {
            const t = session.tabs.find((x) => x.path === path);
            if (!t || !t.saved) return;
            const dirty = !s.doc.eq(t.saved);
            if (dirty !== t.dirty) {
              t.dirty = dirty;
              redraw();
            }
          },
        });
        t.saved = state.doc;
        session.states.set(path, state);
        languageFor(path).then((lang) => {
          if (!lang) return;
          const effects = languageSlot.reconfigure(lang);
          if (session.active === path && view.current) view.current.dispatch({ effects });
          else {
            const s = session.states.get(path);
            if (s) session.states.set(path, s.update({ effects }).state);
          }
        });
      }
      session.tabs.push(t);
    }
    if (!show) return;
    // Reveal it in the tree.
    for (let i = path.indexOf("/"); i > 0; i = path.indexOf("/", i + 1)) session.expanded.add(path.slice(0, i));
    activate(path);
  };

  const sessionById = (id: string | null) => sessions.find((s) => s.id === id);

  /** Starts a Claude or a terminal in the workspace and shows its tab. */
  const newSession = async (kind: SessionKind, prompt?: string, label?: string, conversation?: boolean) => {
    const created = project
      ? await api.openProjectSession(project.name, { kind, prompt, label, ...(conversation ? { conversation } : {}) })
      : await api.openSession(owner, { kind, prompt, label });
    setStarting(created.id);
    activate(sessionTab(created.id));
    onChanged();
  };
  const showSession = (id?: string) => {
    const s = id ? sessionById(id) : sessions.find((x) => x.primary);
    if (s) activate(sessionTab(s.id));
  };

  // After a handoff the new primary's terminal is the one to look at.
  const primaryId = task?.saving_mode ? (sessions.find((s) => s.primary)?.id ?? null) : null;
  const lastPrimary = useRef({ owner, primaryId });
  useEffect(() => {
    const before = lastPrimary.current;
    lastPrimary.current = { owner, primaryId };
    // Only a new primary of the same task: another task's is not a handoff.
    if (before.owner === owner && before.primaryId && primaryId && before.primaryId !== primaryId) {
      activate(sessionTab(primaryId));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [owner, primaryId]);

  const setSaving = (on: boolean) =>
    api
      .setSavingMode(owner, on)
      .then(onChanged)
      .catch((e) => setError((e as Error).message));

  const [autopilotBusy, setAutopilotBusy] = useState(false);
  const setAutopilotMode = (action: "on" | "off" | "resume") => {
    setAutopilotBusy(true);
    api
      .setAutopilot(owner, action)
      .then(onChanged)
      .catch((e) => setError((e as Error).message))
      .finally(() => setAutopilotBusy(false));
  };

  const openChanges = () => {
    session.changesOpen = true;
    activate(CHANGES);
  };

  // Tabs in strip order: the task, its sessions, the changes, the files.
  const order = () => [
    OVERVIEW,
    OFFICE,
    ...sessions.map((s) => sessionTab(s.id)),
    ...(starting && !sessionById(starting) ? [sessionTab(starting)] : []),
    ...session.tabs.map((t) => t.path),
    ...(session.changesOpen ? [CHANGES] : []),
    ...(prTab ? [PRS] : []),
  ];

  const close = async (id: string, force = false) => {
    if (id === OVERVIEW || id === OFFICE) return;
    const strip = order();
    const at = strip.indexOf(id);
    const sid = sessionOf(id);
    if (sid) {
      const s = sessionById(sid);
      if (!s) return;
      if (s.primary) {
        setError(t("La sesión principal no se cierra: termina con la tarea (Marcar como hecha)."));
        return;
      }
      // Closing a working Claude stops it: ask first. A terminal just closes.
      if (s.kind === "claude" && s.live && !force) return setClosingSession(s);
      setClosingSession(null);
      try {
        await api.closeSession(s.id);
      } catch (e) {
        setError((e as Error).message);
        return;
      }
      onChanged();
    } else if (id === CHANGES) session.changesOpen = false;
    else {
      const t = tab(id);
      if (!t) return;
      if (t.dirty && !force) return setClosing(id);
      if (session.active === id && view.current) session.states.set(id, view.current.state);
      session.tabs.splice(session.tabs.indexOf(t), 1);
      session.states.delete(id);
    }
    setClosing(null);
    if (session.active === id) {
      session.active = OVERVIEW;
      activate(strip[at + 1] ?? strip[at - 1] ?? OVERVIEW);
    } else {
      persist(owner, session);
      redraw();
    }
  };

  // A Markdown file shows its source or the rendered document. The editor state stays as
  // it is, so going back keeps the cursor, the unsaved edits and the undo history.
  const togglePreview = () => {
    const t = tab(session.active);
    if (!t || !t.saved || !isMarkdown(t.path)) return;
    if (view.current) session.states.set(t.path, view.current.state);
    t.preview = !t.preview;
    persist(owner, session);
    redraw();
    // Once the editor is shown again.
    if (!t.preview) requestAnimationFrame(() => view.current?.focus());
  };

  // The files that were open last time, loaded in their tab order.
  useEffect(() => {
    const paths = session.pending.splice(0);
    if (paths.length === 0) return;
    (async () => {
      for (const p of paths) await openFile(p, false);
      if (tab(session.active)) activate(session.active);
      else redraw();
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  // One CodeMirror view, its state swapped as tabs change.
  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({ parent: host.current });
    view.current = v;
    const state = session.states.get(session.active);
    if (state) v.setState(state);
    return () => {
      if (session.states.has(session.active)) session.states.set(session.active, v.state);
      v.destroy();
      view.current = null;
    };
  }, [session]);

  // ⌘S lives in the editor's keymap, and the editor is hidden while a preview shows:
  // the preview saves too, so its "sin guardar · ⌘S" stays true.
  const previewing = !!tab(session.active)?.preview;
  useEffect(() => {
    if (!previewing) return;
    const onKey = (e: KeyboardEvent) => {
      if (comboFromEvent(e) !== "Mod+S") return;
      e.preventDefault();
      saveRef.current(session.active);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [previewing, session]);

  // The app sends the shortcuts that act on tabs: ⌘W closes the active one, ⌘T and
  // ⌘⇧T start a terminal or a Claude, ⌃Tab moves between tabs, ⌘B toggles the tree,
  // ⌘⇧V toggles a Markdown file's preview.
  const closeRef = useRef(close);
  closeRef.current = close;
  const previewRef = useRef(togglePreview);
  previewRef.current = togglePreview;
  const newRef = useRef(newSession);
  newRef.current = newSession;
  const cycleRef = useRef<(step: number) => void>(() => {});
  cycleRef.current = (step: number) => {
    const strip = order();
    const at = Math.max(0, strip.indexOf(shown));
    activate(strip[(at + step + strip.length) % strip.length]);
  };
  useEffect(() => {
    const onNew = (e: Event) => {
      const kind = (e as CustomEvent<SessionKind>).detail;
      newRef.current(kind).catch((err) => setError((err as Error).message));
    };
    const onCycle = (e: Event) => cycleRef.current((e as CustomEvent<number>).detail);
    const onFind = () => setFinding(true);
    window.addEventListener("delilah:new-session", onNew);
    window.addEventListener("delilah:cycle-tab", onCycle);
    window.addEventListener("delilah:quick-open", onFind);
    return () => {
      window.removeEventListener("delilah:new-session", onNew);
      window.removeEventListener("delilah:cycle-tab", onCycle);
      window.removeEventListener("delilah:quick-open", onFind);
    };
  }, []);
  useEffect(() => {
    const onClose = () => closeRef.current(shownRef.current);
    const onTree = () =>
      setShowTree((open) => {
        setTreeOpen(!open);
        return !open;
      });
    const onPreview = () => previewRef.current();
    window.addEventListener("delilah:close-tab", onClose);
    window.addEventListener("delilah:toggle-tree", onTree);
    window.addEventListener("delilah:toggle-preview", onPreview);
    return () => {
      window.removeEventListener("delilah:close-tab", onClose);
      window.removeEventListener("delilah:toggle-tree", onTree);
      window.removeEventListener("delilah:toggle-preview", onPreview);
    };
  }, [session]);

  // Follow the disk: reload the active file if the agent changed it, or flag the
  // conflict if it has unsaved edits here.
  const check = async () => {
    const t = tab(session.active);
    if (!t || t.conflict) return;
    try {
      const f = await api.file(owner, t.path);
      if (f.version === t.version || tab(t.path) !== t) return;
      if (t.dirty) t.conflict = true;
      else setText(t.path, f);
      redraw();
    } catch {
      // Deleted, or the daemon is away: leave the tab as it is.
    }
  };
  // The app re-renders every couple of seconds: the timer must not restart with it.
  const checkRef = useRef(check);
  checkRef.current = check;
  useEffect(() => {
    const run = () => checkRef.current();
    const timer = setInterval(run, WATCH_MS);
    window.addEventListener("focus", run);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", run);
    };
  }, []);

  // What the branch changed, to mark it in the tree.
  useEffect(() => {
    if (!hasWorkspace) return;
    let alive = true;
    api
      .changes(owner)
      .then((repos) => {
        if (!alive) return;
        const m = new Map<string, ChangeKind>();
        for (const r of repos) for (const f of r.files) m.set(r.repo === "." ? f.path : `${r.repo}/${f.path}`, f.kind);
        setChanged(m);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [owner, treeRefresh, hasWorkspace]);

  const keepMine = async () => {
    if (!active) return;
    const f = await api.file(owner, active.path);
    active.version = f.version;
    active.conflict = false;
    await save(active.path);
    view.current?.focus();
  };
  const takeDisk = async () => {
    if (!active) return;
    setText(active.path, await api.file(owner, active.path));
    redraw();
  };

  // The task's view setting (FR-007) is whatever the user last picked with the switch.
  const rememberView = (v: ViewMode) => {
    if (task && task.view !== v) api.updateTask(owner, { view: v }).then(onChanged, () => {});
  };
  // A project session's view only changes what is shown (the switch is not offered there).
  const chooseView = (s: Session, v: ViewMode) => {
    showView(s.id, v);
    rememberView(v);
  };

  const name = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const toggleTree = () => window.dispatchEvent(new Event("delilah:toggle-tree"));
  const phase = task ? headerPhaseOf(task, spec) : null;
  const openTerminal = () => {
    newSession("shell").catch((e) => setError((e as Error).message));
  };
  // The tree gives its room to the diff while the changes show.
  const treeVisible = showTree && hasWorkspace && shown !== CHANGES;

  const tabItems: TaskTabItem[] = [
    { id: OVERVIEW, kind: project ? "project" : "task", label: project ? t("Proyecto") : t("Tarea") },
    { id: OFFICE, kind: "office", label: t("Oficina"), attention: sessions.some((s) => sessionDot(s) === "wait") },
    ...sessions.map((s): TaskTabItem => ({
      id: sessionTab(s.id),
      kind: "session",
      label: s.primary ? t("Claude principal") : s.label,
      title: s.label,
      dot: sessionDot(s),
      closable: !s.primary,
    })),
    ...(starting && !sessionById(starting) ? [{ id: sessionTab(starting), kind: "session" as const, label: t("Iniciando…") }] : []),
    ...session.tabs.map((f): TaskTabItem => ({ id: f.path, kind: "file", label: name(f.path), title: f.path, dirty: f.dirty, closable: true })),
    ...(session.changesOpen ? [{ id: CHANGES, kind: "changes" as const, label: t("Cambios"), count: changed.size, closable: true }] : []),
    ...(prTab ? [{ id: PRS, kind: "pr" as const, label: "PR" }] : []),
  ];

  // Chat | Terminal for the session of the open tab: not on the Tarea tab nor plain terminals.
  // Not for a project's sessions: the daemon refuses to change their runtime.
  const turnRunning =
    !!shownClaude && (shownClaude.state === "working" || asks.some((a) => a.session?.id === shownClaude.id && a.ask));
  const sessionSwitch = shownClaude && !project ? (
    <SessionViewSwitch
      session={shownClaude}
      busy={turnRunning}
      onChanged={() => {
        setError(null);
        onChanged();
      }}
      onError={setError}
    />
  ) : undefined;
  const pr = prs[0] ? { number: prs[0].number, snapshot: prs[0].snapshot } : null;

  return (
    <>
      {task ? (
        <TaskHeader
          task={task}
          phase={phase}
          owner={owner}
          hasWorkspace={hasWorkspace}
          showTree={treeVisible}
          onToggleTree={toggleTree}
          sessionSwitch={sessionSwitch}
          autopilotBusy={autopilotBusy}
          onAutopilot={setAutopilotMode}
          onSavingMode={setSaving}
          pr={pr}
          doneAt={task?.finished_at}
          onChanged={onChanged}
          onError={setError}
        />
      ) : (
        <div className="ts-head" data-tauri-drag-region>
          <Icon name="folder" />
          <h1 data-tauri-drag-region className="trunc">{project?.name}</h1>
          <span className="mono muted trunc ts-grow">{project?.path}</span>
          {sessionSwitch}
          <IconButton
            label={t("Árbol de archivos")}
            title={t("Árbol de archivos ({key})", { key: keyOf("toggle-tree") })}
            icon="sidebar"
            pressed={treeVisible}
            onClick={toggleTree}
          />
        </div>
      )}
      <div
        className={`tscreen ${treeVisible ? "with-tree" : ""}`}
        style={{ "--tree-w": `${treeW}px` } as CSSProperties}
      >
        <div className="tmain">
          {task && <SessionChain task={task} sessions={sessions} onOpenSession={showSession} onChanged={onChanged} />}
          <BaseStaleBars owner={owner} notices={notices} onChanged={onChanged} />
          <TaskTabs
            tabs={tabItems}
            active={shown}
            onSelect={activate}
            onClose={(id) => void close(id)}
            onNewSession={(kind) => {
              newSession(kind === "terminal" ? "shell" : "claude").catch((e) => setError((e as Error).message));
            }}
          />
          {error && <div className="error selectable ts-error" role="alert">{error}</div>}

          {shown === OVERVIEW && project && (
            <ProjectView
              project={project}
              sessions={sessions}
              tasks={tasks}
              onOpenTask={onOpenTask}
              onOpenSession={showSession}
              onNewSession={newSession}
              onNewIdea={() => newSession("claude", undefined, undefined, true)}
              onNewTask={onNewTask}
            />
          )}
          {shown === OVERVIEW && task && (
            <TaskView
              task={task}
              sessions={sessions}
              asks={asks}
              onOpenSession={showSession}
              onNewSession={newSession}
              onChanged={onChanged}
              onViewChanges={openChanges}
              onOpenTask={onOpenTask}
            />
          )}
          {shown === OFFICE && <OfficeTab owner={owner} online={online} onOpenSession={showSession} />}
          {shown === PRS && <PrView owner={owner} onOpenTerminal={openTerminal} />}
          {shown === CHANGES && <ChangesView owner={owner} onEdit={openFile} />}

          {closingSession && (
            <div className="ebar warn">
              <span className="ts-grow">
                {t("Cerrar «{label}» detiene ese Claude. Su conversación no se podrá reabrir desde aquí.", { label: closingSession.label })}
              </span>
              <button className="btn btn-p" onClick={() => close(sessionTab(closingSession.id), true)}>{t("Cerrar")}</button>
              <button className="btn btn-ghost" onClick={() => setClosingSession(null)}>{t("Cancelar")}</button>
            </div>
          )}
          {shownSession && !sessionById(shownSession) && <div className="empty">{t("Arrancando la sesión…")}</div>}
          {sessions
            .filter((s) => mounted.has(s.id))
            .map((s) => {
              const on = shownSession === s.id;
              const sessionAsks = asks.filter((a) => a.session?.id === s.id && a.ask);
              const asConversation = viewOf(s) === "chat" && s.kind !== "shell";
              const reopen = () =>
                api
                  .reopenSession(s.id)
                  .then(onChanged)
                  .catch((e) => setError((e as Error).message));
              return (
                <div key={s.id} className={`tterm ts-term${on ? "" : " off"}`}>
                  {s.kind === "setup" && s.live && task && (
                    <SetupBanner
                      task={task}
                      running={isSetupRunning(sessions)}
                      onChanged={onChanged}
                      onOpenTerminal={openTerminal}
                      onOpenSettings={onOpenSettings && task.project ? () => onOpenSettings(task.project!) : undefined}
                    />
                  )}
                  {!asConversation && s.read_only && s.kind === "claude" && s.live && (
                    <div className="ebar warn">
                      <Icon name="lock" />
                      <span className="ts-grow">
                        <b>{t("Solo lectura.")}</b> {t("Este Claude lee, busca y explica, pero no edita.")}
                      </span>
                      <button
                        className="btn"
                        onClick={() =>
                          api
                            .allowEdits(s.id)
                            .then(onChanged)
                            .catch((e) => setError((e as Error).message))
                        }
                      >
                        {t("Permitir cambios en esta sesión")}
                      </button>
                      {onConvert && (
                        <button className="btn" onClick={() => onConvert(s)}>
                          {t("Convertir en tarea")}
                        </button>
                      )}
                    </div>
                  )}
                  {!asConversation && !s.read_only && s.kind === "claude" && s.live && project && onConvert && (
                    <div className="ebar">
                      <span className="ts-grow muted">
                        {t("Claude sobre tu checkout, sin tarea.")}
                      </span>
                      <button className="btn" onClick={() => onConvert(s)}>
                        {t("Convertir en tarea")}
                      </button>
                    </div>
                  )}
                  {!s.live && !asConversation && (
                    <div className="ebar">
                      <span className="ts-grow">{t("Esta sesión no está en marcha (el daemon se reinició o el proceso terminó).")}</span>
                      <button
                        className="btn btn-p"
                        onClick={reopen}
                      >
                        {t("Reabrir")}
                      </button>
                    </div>
                  )}
                  {/* Fixed order, placed by CSS: moving the questions never remounts the
                      terminal or loses an answer being written. */}
                  <div className={`tterm-body ask-${askPosition}`}>
                    {/* A new connection once it comes back to life. */}
                    {asConversation ? (
                      <ConversationView
                        session={s}
                        onOpenTerminal={() => chooseView(s, "terminal")}
                        onOpenFile={(path) => void openFile(path)}
                        onOpenTask={(key) => onOpenTask?.(key)}
                        onReopen={reopen}
                      />
                    ) : (
                      <TerminalPane key={`${s.id}:${s.live}:${s.read_only}`} sessionId={s.id} kind={s.kind} focused={on} />
                    )}
                    {!asConversation && sessionAsks.length > 0 && (
                      <div className="ask-bar">
                        <div className="sec">{t("El agente te pregunta")}</div>
                        {sessionAsks.map((a) => (
                          <AskCard key={a.ask!.id} ask={a.ask!} projectSettings={project?.settings} onDone={onChanged} onOpenTask={onOpenTask} />
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}

          {active && (
            <div className="epath mono muted">
              <span className="selectable trunc">{active.path}</span>
              {active.dirty && <span className="c-wait">{t("sin guardar · ⌘S")}</span>}
              {active.saved && isMarkdown(active.path) && (
                <button
                  className={`btn btn-ghost ts-preview ${active.preview ? "is-on" : ""}`}
                  onClick={togglePreview}
                  title={t("Alternar vista previa ({key})", { key: keyOf("toggle-preview") })}
                  aria-pressed={active.preview}
                >
                  {active.preview ? t("Código") : t("Vista previa")}
                </button>
              )}
            </div>
          )}
          {closing && (
            <div className="ebar">
              <span className="ts-grow">
                {t("«{name}» tiene cambios sin guardar.", { name: name(closing) })}
              </span>
              <button className="btn btn-p" onClick={async () => (await save(closing), close(closing, true))}>{t("Guardar")}</button>
              <button className="btn" onClick={() => close(closing, true)}>{t("Descartar")}</button>
              <button className="btn btn-ghost" onClick={() => setClosing(null)}>{t("Cancelar")}</button>
            </div>
          )}
          {active?.conflict && (
            <div className="ebar warn">
              <span className="ts-grow">
                {t("El archivo ha cambiado en disco (probablemente el agente) y aquí tienes cambios sin guardar.")}
              </span>
              <button className="btn" onClick={takeDisk}>{t("Cargar el del disco")}</button>
              <button className="btn" onClick={keepMine}>{t("Guardar el mío encima")}</button>
            </div>
          )}
                    {/* Always mounted: one CodeMirror view for every file tab. */}
          <div className={`ehost${active?.saved && !active.preview ? "" : " ts-off"}`} ref={host} />
          {/* The live text, unsaved edits included; a reload from disk redraws it. */}
          {active?.saved && active.preview && (
            <div className="md-preview">
              <Markdown
                text={(stateOf(active.path)?.doc ?? active.saved).toString()}
                doc={{
                  path: active.path,
                  version: active.version,
                  openFile,
                  loadImage: (path) => api.image(owner, path),
                }}
              />
            </div>
          )}
          {active && !active.saved && (
            <div className="empty">
              <span>
                {active.size > 1 << 20 ? t("Este archivo es demasiado grande para editarlo aquí.") : t("Este archivo es binario y no se puede editar aquí.")}
              </span>
            </div>
          )}
        </div>

        {finding && hasWorkspace && <QuickOpen taskKey={owner} onOpen={openFile} onClose={() => setFinding(false)} />}
        {treeVisible && (
          <aside id={treeId} className="tpanel" aria-label={t("Archivos del workspace")}>
            <TreeResizer width={treeW} controls={treeId} onChange={setTreeW} />
            <div className="tpanel-head">
              <span className="sec">{t("Archivos")}</span>
              <button className="btn btn-ghost icon-btn mono" onClick={() => setFinding(true)} title={t("Abrir un archivo por su nombre")}>
                {keyOf("quick-open")}
              </button>
              <button className="btn btn-ghost icon-btn" onClick={() => setTreeRefresh((n) => n + 1)} title={t("Recargar")} aria-label={t("Actualizar el árbol")}>
                <Icon name="refresh" />
              </button>
              <button className="btn btn-ghost icon-btn" onClick={toggleTree} title={t("Ocultar ({key})", { key: keyOf("toggle-tree") })} aria-label={t("Ocultar el árbol")}>
                <Icon name="close" />
              </button>
            </div>
            <FileTree
              taskKey={owner}
              changed={changed}
              active={tab(session.active) ? session.active : null}
              expanded={session.expanded}
              refresh={treeRefresh}
              onOpen={openFile}
            />
          </aside>
        )}
      </div>
    </>
  );
}
