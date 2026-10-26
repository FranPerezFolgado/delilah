import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import {
  api, ApiError, DEFAULT_JQL, type JiraConnect, type JiraIssue, type JiraMode, type JiraProgress, type JiraSettings,
  type JiraSprintFilter, type JiraWho, type Task, type TaskStatus,
} from "../api";
import { Button, Icon, IconButton, Menu, Segmented, Skeleton, StatusBadge } from "./ui";
import { Markdown } from "./Markdown";
import { openExternal } from "../platform";
import { t, tn } from "../i18n";
import { useBreakpoints } from "../useWidth";
import "./JiraView.css";

interface Props {
  onAnalyze: (issue: JiraIssue) => void;
  onOpenTask: (key: string) => void;
}

type GroupBy = "status" | "epic";

/** A top-level row and the sub-tasks that hang from it. */
interface Node {
  issue: JiraIssue;
  children: JiraIssue[];
}

interface Group {
  id: string;
  label: string;
  hint?: string;
  category?: string;
  nodes: Node[];
}

const CATEGORY_ORDER = ["indeterminate", "new", "done"];
const categoryRank = (c: string) => {
  const i = CATEGORY_ORDER.indexOf(c);
  return i < 0 ? 1.5 : i;
};

/** Sub-tasks under their parent when it is listed; the rest, orphans included, at the top. */
function nest(issues: JiraIssue[]): Node[] {
  const listed = new Set(issues.map((i) => i.key));
  const children = new Map<string, JiraIssue[]>();
  const top: JiraIssue[] = [];
  for (const i of issues) {
    const parent = i.parent?.key;
    if (parent && parent !== i.key && listed.has(parent)) {
      const kids = children.get(parent) ?? [];
      kids.push(i);
      children.set(parent, kids);
    } else {
      top.push(i);
    }
  }
  return top.map((issue) => ({ issue, children: children.get(issue.key) ?? [] }));
}

const haystack = (i: JiraIssue) =>
  `${i.key} ${i.summary} ${i.components.join(" ")} ${i.epic?.key ?? ""} ${i.epic?.summary ?? ""} ${i.parent?.key ?? ""} ${i.parent?.summary ?? ""}`
    .toLowerCase();

/** A parent stays if it or a sub-task matches; a matching parent keeps all its sub-tasks. */
function filterTree(nodes: Node[], q: string): Node[] {
  if (!q) return nodes;
  return nodes.flatMap((n) => {
    if (haystack(n.issue).includes(q)) return [n];
    const children = n.children.filter((c) => haystack(c).includes(q));
    return children.length ? [{ issue: n.issue, children }] : [];
  });
}

/** Sections of the list: one per status, in progress first; or one per epic. A sub-task goes with its parent. */
function groupIssues(nodes: Node[], by: GroupBy): Group[] {
  const groups = new Map<string, Group>();
  for (const n of nodes) {
    const i = n.issue;
    const id = by === "status" ? i.status : i.epic?.key ?? "";
    let g = groups.get(id);
    if (!g) {
      g = by === "status"
        ? { id, label: i.status || "—", category: i.status_category, nodes: [] }
        : i.epic
          ? { id, label: i.epic.summary ?? i.epic.key, hint: i.epic.summary ? i.epic.key : undefined, nodes: [] }
          : { id, label: t("Sin épica"), nodes: [] };
      groups.set(id, g);
    }
    g.nodes.push(n);
  }
  const list = [...groups.values()];
  if (by === "status") {
    list.sort((a, b) => categoryRank(a.category ?? "") - categoryRank(b.category ?? ""));
  } else {
    // Inside an epic, the work in progress first; the tickets with no epic go last.
    const byCategory = (a: JiraIssue, b: JiraIssue) => categoryRank(a.status_category) - categoryRank(b.status_category);
    for (const g of list) {
      g.nodes = g.nodes
        .map((n) => ({ issue: n.issue, children: [...n.children].sort(byCategory) }))
        .sort((a, b) => byCategory(a.issue, b.issue));
    }
    list.sort((a, b) => Number(a.id === "") - Number(b.id === ""));
  }
  return list;
}

const initials = (name: string) =>
  name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0]!.toUpperCase()).join("");

/** Only the two top priorities get a mark in the list. */
const isUrgent = (p: string | null) => !!p && /^(alta|cr[ií]tica|high|highest|critical|blocker|bloqueante)$/i.test(p.trim());

const isTokenRejected = (e: unknown) =>
  e instanceof ApiError ? e.status === 401 : /\b401\b/.test((e as Error)?.message ?? "");

function StatusDot({ category }: { category: string }) {
  return <span className={`jdot jdot-${category || "new"}`} aria-hidden />;
}

/** Below this window width the three filter chips collapse into one «Filtros · N» menu. */
const FILTERS_COLLAPSE_BELOW = 1024;
const narrowFilters = () =>
  typeof window !== "undefined" && !!window.matchMedia?.(`(max-width: ${FILTERS_COLLAPSE_BELOW - 1}px)`).matches;

function useCollapsedFilters(): boolean {
  const [collapsed, setCollapsed] = useState(narrowFilters);
  useEffect(() => {
    if (!window.matchMedia) return;
    const mq = window.matchMedia(`(max-width: ${FILTERS_COLLAPSE_BELOW - 1}px)`);
    const update = () => setCollapsed(mq.matches);
    mq.addEventListener("change", update);
    update();
    return () => mq.removeEventListener("change", update);
  }, []);
  return collapsed;
}

/** Tickets from Jira, and "Analizar" to start a task on one. */
export function JiraView({ onAnalyze, onOpenTask }: Props) {
  const [settings, setSettings] = useState<JiraSettings | null | undefined>(undefined);
  const [editing, setEditing] = useState(false);
  const [issues, setIssues] = useState<JiraIssue[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [who, setWho] = useState<JiraWho>("assigned");
  const [progress, setProgress] = useState<JiraProgress>("open");
  const [sprint, setSprint] = useState<JiraSprintFilter>("all");
  const [groupBy, setGroupBy] = useState<GroupBy>("status");
  const [folded, setFolded] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [expired, setExpired] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);
  // Below 1200 px the detail is a drawer that only opens when a row is chosen.
  const { overlays } = useBreakpoints();
  const collapsed = useCollapsedFilters();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const showDrawer = overlays && drawerOpen;
  useEffect(() => {
    if (!showDrawer) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setDrawerOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [showDrawer]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    setExpired(false);
    try {
      const rows = await api.jiraIssues(who, progress, sprint);
      setIssues(rows);
      setSelected((s) => (s && rows.some((r) => r.key === s) ? s : rows[0]?.key ?? null));
    } catch (e) {
      if (isTokenRejected(e)) setExpired(true);
      else setError((e as Error).message);
    } finally {
      setLoading(false);
      setLoaded(true);
    }
  }, [who, progress, sprint]);

  useEffect(() => {
    api.jira().then((r) => setSettings(r.settings)).catch((e) => setError(e.message));
  }, []);
  useEffect(() => {
    if (settings) load();
  }, [settings, load]);

  const hasQuery = !!settings && settings.jql.trim() !== DEFAULT_JQL;
  useEffect(() => {
    if (settings && !hasQuery && who === "query") setWho("assigned");
  }, [settings, hasQuery, who]);

  const shown = useMemo(() => filterTree(nest(issues), filter.trim().toLowerCase()), [issues, filter]);
  const groups = useMemo(() => groupIssues(shown, groupBy), [shown, groupBy]);
  const issue = issues.find((i) => i.key === selected);

  const toggle = (id: string) =>
    setFolded((f) => {
      const next = new Set(f);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  if (settings === undefined) {
    return error
      ? <div className="empty"><span className="error selectable">{error}</span></div>
      : <div className="jv-loading"><Skeleton lines={4} /></div>;
  }
  if (settings === null || editing) {
    return (
      <ConnectForm
        current={settings}
        onCancel={settings ? () => setEditing(false) : undefined}
        onConnected={(s) => {
          setSettings(s);
          setEditing(false);
          setSelected(null);
        }}
      />
    );
  }

  const WHO: { value: JiraWho; label: string }[] = [
    { value: "assigned", label: t("Asignados a mí") },
    { value: "reported", label: t("Reportados por mí") },
    ...(hasQuery ? [{ value: "query" as const, label: t("Mi consulta") }] : []),
  ];
  const PROGRESS: { value: JiraProgress; label: string }[] = [
    { value: "open", label: t("Abiertos") },
    { value: "done", label: t("Hechos") },
  ];
  const SPRINT: { value: JiraSprintFilter; label: string }[] = [
    { value: "all", label: t("Todos") },
    { value: "current", label: t("Sprint actual") },
  ];
  const chip = <T extends string>(label: string, options: { value: T; label: string }[], value: T, set: (v: T) => void) => (
    <Menu
      items={options.map((o) => ({ label: o.label, onSelect: () => set(o.value) }))}
      trigger={(p) => (
        <button type="button" className="jv-chip" {...p}>
          <span className="jv-chip-k">{label}</span>
          <span className="jv-chip-v">{options.find((o) => o.value === value)?.label}</span>
          <Icon name="chevron" className="jv-chip-c" />
        </button>
      )}
    />
  );
  const activeFilters = Number(who !== "assigned") + Number(progress !== "open") + Number(sprint !== "all");
  const filterItems = <T extends string>(label: string, options: { value: T; label: string }[], value: T, set: (v: T) => void) =>
    options.map((o) => ({ label: `${o.value === value ? "✓ " : ""}${label}: ${o.label}`, onSelect: () => set(o.value) }));
  const filtersMenu = (
    <Menu
      items={[
        ...filterItems(t("De quién"), WHO, who, setWho),
        ...filterItems(t("Estado"), PROGRESS, progress, setProgress),
        ...filterItems(t("Sprint"), SPRINT, sprint, setSprint),
      ]}
      trigger={(p) => (
        <button type="button" className="jv-chip" {...p}>
          <span className="jv-chip-v">{t("Filtros · {n}", { n: activeFilters })}</span>
          <Icon name="chevron" className="jv-chip-c" />
        </button>
      )}
    />
  );
  const clearFilters = () => {
    setWho("assigned");
    setProgress("open");
    setSprint("all");
    setFilter("");
  };

  const avatars = who !== "assigned";
  const row = (i: JiraIssue, child: boolean) => {
    const meta = [
      !child && i.epic ? i.epic.summary ?? i.epic.key : null,
      !child && i.sprint ? i.sprint.name : null,
      !child && (i.subtask_count ?? 0) > 0 ? t("{done}/{total} subtareas", { done: i.subtasks_done ?? 0, total: i.subtask_count ?? 0 }) : null,
      groupBy === "epic" ? i.status : null,
    ].filter(Boolean);
    return (
      <button key={i.key} type="button" className={`jv-row ${child ? "child" : ""} ${i.key === selected ? "on" : ""}`}
        aria-current={i.key === selected ? "true" : undefined} onClick={() => {
          setSelected(i.key);
          setDrawerOpen(true);
        }}>
        <StatusDot category={i.status_category} />
        <span className="jv-main">
          <span className="jv-l1">
            <span className="mono jv-key">{i.key}</span>
            <span className="jv-title">{i.summary}</span>
            {isUrgent(i.priority) && (
              <span className="jv-prio"><Icon name="priority" />{i.priority}</span>
            )}
          </span>
          <span className="jv-l2">{meta.join(" · ")}</span>
        </span>
        {i.task_status && <span className="jv-linked" title={t("Tiene una tarea en Delilah")}><Icon name="tasks" /></span>}
        {avatars && (i.assignee
          ? <span className="javatar" title={i.assignee}>{initials(i.assignee)}</span>
          : <span className="javatar none" title={t("Sin asignar")} />)}
      </button>
    );
  };

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>Jira</h1>
        <span className="muted jv-sub">
          {settings.mode === "sample" ? t("datos de ejemplo") : settings.site} · {tn(issues.length, "{n} ticket", "{n} tickets")}
        </span>
        <input className="input jv-search" placeholder={t("Filtrar tickets…")}
          value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("Filtrar tickets")} />
        <IconButton label={t("Recargar")} icon="refresh" onClick={load} disabled={loading} />
        <Button variant="secondary" onClick={() => setEditing(true)}>{t("Conexión")}</Button>
      </div>
      <div className="jv-bar">
        {collapsed ? filtersMenu : (
          <>
            {chip(t("De quién"), WHO, who, setWho)}
            {chip(t("Estado"), PROGRESS, progress, setProgress)}
            {chip(t("Sprint"), SPRINT, sprint, setSprint)}
          </>
        )}
        <span className="jv-bar-end">
          <span className="muted">{t("Agrupar por")}</span>
          <Segmented mode="radio" label={t("Agrupar por")} value={groupBy} onChange={(v) => setGroupBy(v as GroupBy)}
            options={[{ value: "status", label: t("Estado") }, { value: "epic", label: t("Épica") }]} />
        </span>
      </div>
      {expired && (
        <div className="jv-banner" role="alert">
          <strong>{t("Jira rechazó el token (401)")}</strong>
          <span>{t("Lo que ves son los últimos datos conocidos.")}</span>
          <Button variant="primary" size={28} onClick={() => setEditing(true)}>{t("Reconectar")}</Button>
        </div>
      )}
      {error && <div className="error selectable jv-error">{error}</div>}
      <div className={`jv-body ${expired ? "stale" : ""}`}>
        <div className="jv-list">
          {!loaded && loading && <div className="jv-loading"><Skeleton lines={4} /></div>}
          {groups.map((g) => {
            const open = !folded.has(`${groupBy}:${g.id}`);
            return (
              <section key={g.id}>
                <button type="button" className="jgroup" onClick={() => toggle(`${groupBy}:${g.id}`)} aria-expanded={open}>
                  <span className={`chev ${open ? "open" : ""}`}>›</span>
                  {g.category !== undefined && <StatusDot category={g.category} />}
                  <span className="jgroup-name trunc">{g.label}</span>
                  {g.hint && <span className="mono muted jv-hint">{g.hint}</span>}
                  <span className="jgroup-count">{g.nodes.length}</span>
                </button>
                {open && g.nodes.map((n) => (
                  <Fragment key={n.issue.key}>
                    {row(n.issue, false)}
                    {n.children.map((c) => row(c, true))}
                  </Fragment>
                ))}
              </section>
            );
          })}
          {shown.length === 0 && !loading && loaded && !error && !expired && (
            <div className="jv-empty">
              <span>{t("Ningún ticket con estos filtros")}</span>
              <Button variant="secondary" size={28} onClick={clearFilters}>{t("Quitar filtros")}</Button>
            </div>
          )}
        </div>
        {issue && (!overlays || showDrawer) && (
          <>
            {overlays && <div className="jv-scrim" onClick={() => setDrawerOpen(false)} />}
            <Detail key={issue.key} issue={issue} expired={expired} parentListed={!!issue.parent && issues.some((i) => i.key === issue.parent!.key)}
              drawer={overlays} onClose={() => setDrawerOpen(false)}
              onSelect={setSelected} onAnalyze={onAnalyze} onOpenTask={onOpenTask} />
          </>
        )}
      </div>
    </>
  );
}

function Detail({ issue, expired, parentListed, drawer, onClose, onSelect, onAnalyze, onOpenTask }: {
  issue: JiraIssue;
  /** The token expired: «Reconectar» is the view's primary action. */
  expired: boolean;
  parentListed: boolean;
  drawer: boolean;
  onClose: () => void;
  onSelect: (key: string) => void;
  onAnalyze: (issue: JiraIssue) => void;
  onOpenTask: (key: string) => void;
}) {
  const [subs, setSubs] = useState<JiraIssue[] | null>(null);
  const [task, setTask] = useState<Task | null>(null);
  const hasSubs = !issue.subtask && (issue.subtask_count ?? 0) > 0;

  useEffect(() => {
    if (!hasSubs) return;
    let live = true;
    api.jiraSubtasks(issue.key).then((r) => live && setSubs(r)).catch(() => live && setSubs([]));
    return () => { live = false; };
  }, [issue.key, hasSubs]);
  useEffect(() => {
    if (!issue.task_status) return;
    let live = true;
    api.tasks().then((r) => live && setTask(r.find((x) => x.key === issue.key) ?? null)).catch(() => {});
    return () => { live = false; };
  }, [issue.key, issue.task_status]);

  const status = task?.status ?? ({ type: issue.task_status } as TaskStatus);
  const done = subs?.filter((s) => s.status_category === "done").length ?? 0;
  const bySubStatus = useMemo(() => {
    const m = new Map<string, { category: string; items: JiraIssue[] }>();
    for (const s of subs ?? []) {
      const g = m.get(s.status) ?? { category: s.status_category, items: [] };
      g.items.push(s);
      m.set(s.status, g);
    }
    return [...m].sort((a, b) => categoryRank(a[1].category) - categoryRank(b[1].category));
  }, [subs]);

  return (
    <aside className={`jv-detail ${drawer ? "jv-drawer" : ""}`} role={drawer ? "dialog" : undefined} aria-label={drawer ? issue.key : undefined}>
      {drawer && (
        <div className="jv-close">
          <Button variant="secondary" size={28} onClick={onClose}>{t("Cerrar")}</Button>
        </div>
      )}
      <div className="jv-dhead">
        <div className="mono muted">{issue.key}</div>
        <h2 className="t-sec selectable">{issue.summary}</h2>
        {issue.url && (
          <Button variant="ghost" size={28} onClick={() => openExternal(issue.url!).catch(() => {})}>
            {t("Abrir en Jira ↗")}
          </Button>
        )}
      </div>
      {issue.task_status ? (
        <button type="button" className="jv-task" onClick={() => onOpenTask(issue.key)}>
          <Icon name="tasks" />
          <span className="jv-task-what">{[task?.phase, t("Tarea en Delilah")].filter(Boolean).join(" · ")}</span>
          <StatusBadge status={status} />
          <span className="b b-s sm jv-task-open">{t("Abrir ›")}</span>
        </button>
      ) : (
        <div className="jv-analyze">
          <Button variant={expired ? "secondary" : "primary"} onClick={() => onAnalyze(issue)}>{t("Analizar con Claude")}</Button>
          <span className="muted">{t("Claude lee el ticket y propone un plan antes de tocar código.")}</span>
        </div>
      )}
      <div className="jv-fields">
        <span className="muted">{t("Tipo")}</span><span>{issue.issue_type}</span>
        <span className="muted">{t("Prioridad")}</span><span>{issue.priority ?? "—"}</span>
        <span className="muted">{t("Estado")}</span>
        <span className="jv-status"><StatusDot category={issue.status_category} />{issue.status}</span>
        {issue.parent && (
          <>
            <span className="muted">{t("Padre")}</span>
            {parentListed ? (
              <button type="button" className="jparent" onClick={() => onSelect(issue.parent!.key)}>
                {issue.parent.summary ?? ""} <span className="mono muted">{issue.parent.key}</span>
              </button>
            ) : (
              <span>{issue.parent.summary ?? ""} <span className="mono muted">{issue.parent.key}</span></span>
            )}
          </>
        )}
        <span className="muted">{t("Épica")}</span>
        <span>{issue.epic ? <>{issue.epic.summary ?? ""} <span className="mono muted">{issue.epic.key}</span></> : "—"}</span>
        <span className="muted">{t("Sprint")}</span><span>{issue.sprint?.name ?? "—"}</span>
        <span className="muted">{t("Asignado")}</span><span>{issue.assignee ?? t("Sin asignar")}</span>
        <span className="muted">{t("Informador")}</span><span>{issue.reporter ?? "—"}</span>
        <span className="muted">{t("Componentes")}</span>
        <span className="mono">{issue.components.length ? issue.components.join(", ") : "—"}</span>
      </div>
      <div>
        <div className="sec">{t("Descripción")}</div>
        {issue.description ? <Markdown text={issue.description} remoteImages="click" /> : <span className="muted">{t("Sin descripción.")}</span>}
      </div>
      {hasSubs && (
        <div>
          <div className="sec jv-subs-head">
            <span>{t("Subtareas")}</span>
            {subs && subs.length > 0 && <span className="jv-progress">{t("{done}/{total} hechas", { done, total: subs.length })}</span>}
          </div>
          {subs === null ? <Skeleton lines={3} /> : bySubStatus.map(([status, g]) => (
            <SubGroup key={status} status={status} category={g.category} items={g.items} />
          ))}
        </div>
      )}
    </aside>
  );
}

/** Only the work in progress starts open. */
function SubGroup({ status, category, items }: { status: string; category: string; items: JiraIssue[] }) {
  const [open, setOpen] = useState(category === "indeterminate");
  return (
    <div className="jv-sg">
      <button type="button" className="jv-sg-head" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className={`chev ${open ? "open" : ""}`}>›</span>
        <StatusDot category={category} />
        <span>{status}</span>
        <span className="jgroup-count">{items.length}</span>
      </button>
      {open && items.map((s) => (
        <div key={s.key} className="jv-sub">
          <span className="mono muted">{s.key}</span>
          <span className="jv-sub-title">{s.summary}</span>
          {s.assignee && <span className="javatar" title={s.assignee}>{initials(s.assignee)}</span>}
        </div>
      ))}
    </div>
  );
}

type Errors = Partial<Record<"site" | "email" | "token", string>>;

function ConnectForm({ current, onConnected, onCancel }: {
  current: JiraSettings | null;
  onConnected: (s: JiraSettings) => void;
  onCancel?: () => void;
}) {
  const [mode, setMode] = useState<JiraMode>(current?.mode ?? "cloud");
  const [site, setSite] = useState(current?.site ?? "");
  const [email, setEmail] = useState(current?.email ?? "");
  const [token, setToken] = useState("");
  const [jql, setJql] = useState(current?.jql ?? "");
  const [errors, setErrors] = useState<Errors>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const keepsToken = !!current?.site && current.site === site.trim();

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const found: Errors = {};
    if (mode !== "sample") {
      if (!site.trim()) found.site = t("Falta la URL del sitio");
      else if (!/^https?:\/\//i.test(site.trim())) found.site = t("La URL debe empezar por https://");
      if (mode === "cloud" && !email.trim()) found.email = t("Falta el email");
      if (!token.trim() && !keepsToken) found.token = t("Falta el token");
    }
    setErrors(found);
    setError(null);
    if (Object.keys(found).length) return;
    setBusy(true);
    const req: JiraConnect = { mode, jql: jql.trim() || undefined };
    if (mode !== "sample") {
      req.site = site.trim();
      req.token = token.trim() || undefined;
      if (mode === "cloud") req.email = email.trim();
    }
    try {
      const res = await api.connectJira(req);
      onConnected(res.settings);
    } catch (err) {
      const fields = err instanceof ApiError ? err.fields : [];
      const mapped: Errors = {};
      for (const f of fields) if (f.field === "site" || f.field === "email" || f.field === "token") mapped[f.field] = f.message;
      if (Object.keys(mapped).length) setErrors(mapped);
      else setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const field = (key: keyof Errors, label: string, input: React.ReactNode) => (
    <label className="field">
      <span>{label}</span>
      {input}
      {errors[key] && <span className="jv-ferr" role="alert">{errors[key]}</span>}
    </label>
  );

  return (
    <>
      <div className="view-head" data-tauri-drag-region><h1 data-tauri-drag-region>{t("Conectar Jira")}</h1></div>
      <div className="view-body">
        <form className="card jv-form" onSubmit={submit} noValidate>
          <Segmented mode="radio" label={t("Tipo de Jira")} value={mode} onChange={(v) => { setMode(v as JiraMode); setErrors({}); }}
            options={[
              { value: "cloud", label: t("Cloud") },
              { value: "data-center", label: t("Server") },
              { value: "sample", label: t("Datos de ejemplo") },
            ]} />
          {mode !== "sample" && (
            <>
              {field("site", t("URL del sitio"),
                <input className="input mono" placeholder={mode === "cloud" ? t("https://tu-empresa.atlassian.net") : t("https://jira.tu-empresa.com")}
                  value={site} onChange={(e) => setSite(e.target.value)} aria-invalid={!!errors.site} />)}
              {mode === "cloud" && field("email", t("Email de tu cuenta de Atlassian"),
                <input className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} aria-invalid={!!errors.email} />)}
              {field("token", `${mode === "cloud" ? "API token" : t("Token de acceso personal")}${keepsToken ? ` ${t("(vacío = mantener el guardado)")}` : ""}`,
                <input className="input mono" type="password" value={token} onChange={(e) => setToken(e.target.value)}
                  autoComplete="off" aria-invalid={!!errors.token} />)}
              <p className="muted jv-note">
                {t("El token se guarda en el Llavero de macOS, no en Delilah.")}
                {mode === "cloud" && t(" Se crea en id.atlassian.com → Seguridad → Tokens de API.")}
              </p>
            </>
          )}
          <label className="field">
            <span>{t("Consulta JQL (opcional)")}</span>
            <input className="input mono" placeholder="assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"
              value={jql} onChange={(e) => setJql(e.target.value)} />
          </label>
          {error && <div className="error selectable">{error}</div>}
          <div className="jv-actions">
            <Button variant="primary" type="submit" disabled={busy}>{busy ? t("Comprobando…") : t("Conectar")}</Button>
            {onCancel && <Button variant="secondary" onClick={onCancel}>{t("Cancelar")}</Button>}
          </div>
        </form>
      </div>
    </>
  );
}
