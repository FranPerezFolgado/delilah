import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import {
  api, DEFAULT_JQL, type JiraConnect, type JiraIssue, type JiraMode, type JiraProgress, type JiraSettings,
  type JiraSprintFilter, type JiraWho,
} from "../api";
import { Icon } from "./Icon";
import { Markdown } from "./Markdown";
import { t, tn } from "../i18n";

interface Props {
  onAnalyze: (issue: JiraIssue) => void;
  onOpenTask: (key: string) => void;
}

const TASK_LABELS: Record<string, string> = {
  queued: "En cola",
  "preparing-workspace": "Preparando",
  running: "En ejecución",
  "waiting-for-user": "Te espera",
  "paused-by-limit": "Pausada",
  done: "Hecha",
  failed: "Fallida",
};

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

function StatusDot({ category }: { category: string }) {
  return <span className={`jdot jdot-${category || "new"}`} aria-hidden />;
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
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const rows = await api.jiraIssues(who, progress, sprint);
      setIssues(rows);
      setSelected((s) => (s && rows.some((r) => r.key === s) ? s : rows[0]?.key ?? null));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
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

  if (settings === undefined) return <div className="empty">{t("Cargando…")}</div>;
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

  const WHO: { id: JiraWho; label: string }[] = [
    { id: "assigned", label: t("Asignados") },
    { id: "reported", label: t("Reportados") },
    ...(hasQuery ? [{ id: "query" as const, label: t("Mi consulta") }] : []),
  ];
  const PROGRESS: { id: JiraProgress; label: string }[] = [
    { id: "open", label: t("Abiertos") },
    { id: "done", label: t("Hechos") },
  ];
  const SPRINT: { id: JiraSprintFilter; label: string }[] = [
    { id: "all", label: t("Todos") },
    { id: "current", label: t("Actual") },
  ];
  const GROUP_BY: { id: GroupBy; label: string }[] = [
    { id: "status", label: t("Estado") },
    { id: "epic", label: t("Épica") },
  ];

  const row = (i: JiraIssue, child: boolean, subs: number) => (
    <button key={i.key} className={`jrow ${child ? "child" : ""} ${i.key === selected ? "on" : ""}`} onClick={() => setSelected(i.key)}>
      <StatusDot category={i.status_category} />
      <span className="mono jkey">{i.key}</span>
      <span className="trunc jtitle">{i.summary}</span>
      {subs > 0 && <span className="jsubs">{tn(subs, "{n} subtarea", "{n} subtareas")}</span>}
      {!child && i.parent && (
        <span className="jtag trunc mono" title={i.parent.summary ?? undefined}>↳ {i.parent.key}</span>
      )}
      {groupBy === "epic" && <span className="jtag">{i.status}</span>}
      {groupBy === "status" && !child && i.epic && (
        <span className="jtag trunc" title={i.epic.key}>{i.epic.summary ?? i.epic.key}</span>
      )}
      {sprint === "all" && !child && i.sprint && <span className="jtag trunc">{i.sprint.name}</span>}
      {i.task_status && (
        <span className="pill">{TASK_LABELS[i.task_status] ? t(TASK_LABELS[i.task_status]) : i.task_status}</span>
      )}
      <span className="muted jmeta">{i.issue_type}</span>
      <span className="muted jmeta">{i.priority ?? "—"}</span>
      {i.assignee
        ? <span className="javatar" title={i.assignee}>{initials(i.assignee)}</span>
        : <span className="javatar none" title={t("Sin asignar")} />}
    </button>
  );
  const parentListed = !!issue?.parent && issues.some((i) => i.key === issue.parent!.key);

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>Jira</h1>
        <span className="muted" style={{ fontSize: 12 }}>
          {settings.mode === "sample" ? t("datos de ejemplo") : settings.site} · {tn(issues.length, "{n} ticket", "{n} tickets")}
        </span>
        <input className="input" style={{ marginLeft: "auto", width: 260, height: 26 }} placeholder={t("Filtrar tickets…")}
          value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("Filtrar tickets")} />
        <button className="btn" onClick={load} disabled={loading} aria-label={t("Recargar")}>
          <Icon name="refresh" />
        </button>
        <button className="btn" onClick={() => setEditing(true)}>{t("Conexión")}</button>
      </div>
      <div className="jbar">
        <Segs label={t("De quién")} options={WHO} value={who} onChange={setWho} />
        <Segs label={t("Estado")} options={PROGRESS} value={progress} onChange={setProgress} />
        <Segs label={t("Sprint")} options={SPRINT} value={sprint} onChange={setSprint} />
        <span className="muted" style={{ marginLeft: "auto", fontSize: 12 }}>{t("Agrupar por")}</span>
        <Segs label={t("Agrupar por")} options={GROUP_BY} value={groupBy} onChange={setGroupBy} />
      </div>
      {error && <div className="error selectable" style={{ padding: "8px 20px" }}>{error}</div>}
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <div style={{ flex: 1, minWidth: 0, overflowY: "auto", padding: "4px 0 16px" }}>
          {groups.map((g) => {
            const open = !folded.has(`${groupBy}:${g.id}`);
            return (
              <section key={g.id}>
                <button className="jgroup" onClick={() => toggle(`${groupBy}:${g.id}`)} aria-expanded={open}>
                  <span className={`chev ${open ? "open" : ""}`}>›</span>
                  {g.category !== undefined && <StatusDot category={g.category} />}
                  <span className="jgroup-name trunc">{g.label}</span>
                  {g.hint && <span className="mono muted" style={{ fontSize: 11 }}>{g.hint}</span>}
                  <span className="jgroup-count">{g.nodes.length}</span>
                </button>
                {open && g.nodes.map((n) => (
                  <Fragment key={n.issue.key}>
                    {row(n.issue, false, n.children.length)}
                    {n.children.map((c) => row(c, true, 0))}
                  </Fragment>
                ))}
              </section>
            );
          })}
          {shown.length === 0 && !loading && (
            <div className="muted" style={{ padding: "12px 20px", fontSize: 12.5 }}>
              {filter ? t("No hay tickets con ese filtro.") : t("No hay tickets para esta consulta.")}
            </div>
          )}
        </div>
        {issue && (
          <aside style={{ width: 460, flex: "none", borderLeft: "1px solid var(--bd)", background: "var(--panel)",
            overflowY: "auto", padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
            <div>
              <div className="mono muted">{issue.key}</div>
              <div style={{ fontSize: 15, fontWeight: 600, marginTop: 2 }} className="selectable">{issue.summary}</div>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "100px 1fr", gap: "6px 10px", fontSize: 12.5 }}>
              <span className="muted">{t("Tipo")}</span><span>{issue.issue_type}</span>
              <span className="muted">{t("Prioridad")}</span><span>{issue.priority ?? "—"}</span>
              <span className="muted">{t("Estado")}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}><StatusDot category={issue.status_category} />{issue.status}</span>
              {issue.parent && (
                <>
                  <span className="muted">{t("Padre")}</span>
                  {parentListed ? (
                    <button className="jparent" onClick={() => setSelected(issue.parent!.key)}>
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
              {issue.task_status ? (
                <button className="btn" onClick={() => onOpenTask(issue.key)}>{t("Ver la tarea en Delilah")}</button>
              ) : (
                <button className="btn btn-p" style={{ height: 30 }} onClick={() => onAnalyze(issue)}>
                  {t("Analizar con Claude")}
                </button>
              )}
            </div>
            <div>
              <div className="sec" style={{ marginBottom: 6 }}>{t("Descripción")}</div>
              {issue.description ? <Markdown text={issue.description} remoteImages="click" /> : <span className="muted">{t("Sin descripción.")}</span>}
            </div>
          </aside>
        )}
      </div>
    </>
  );
}

function Segs<T extends string>({ label, options, value, onChange }: {
  label: string;
  options: { id: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="segs" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.id} role="radio" aria-checked={value === o.id} className={`seg ${value === o.id ? "on" : ""}`}
          onClick={() => onChange(o.id)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

const MODES: { id: JiraMode; label: string }[] = [
  { id: "cloud", label: "Jira Cloud" },
  { id: "data-center", label: "Jira Server / Data Center" },
  { id: "sample", label: "Datos de ejemplo" },
];

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
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
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
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="view-head" data-tauri-drag-region><h1 data-tauri-drag-region>{t("Conectar Jira")}</h1></div>
      <div className="view-body">
        <form className="card" style={{ padding: 16, display: "flex", flexDirection: "column", gap: 12, maxWidth: 620 }} onSubmit={submit}>
          <div className="choices" role="radiogroup" aria-label={t("Tipo de Jira")}>
            {MODES.map((m) => (
              <label key={m.id} className={`choice ${mode === m.id ? "on" : ""}`}>
                <input type="radio" name="mode" checked={mode === m.id} onChange={() => setMode(m.id)} /> {t(m.label)}
              </label>
            ))}
          </div>
          {mode !== "sample" && (
            <>
              <label className="field">
                <span>{t("URL del sitio")}</span>
                <input className="input mono" placeholder={mode === "cloud" ? t("https://tu-empresa.atlassian.net") : t("https://jira.tu-empresa.com")}
                  value={site} onChange={(e) => setSite(e.target.value)} required />
              </label>
              {mode === "cloud" && (
                <label className="field">
                  <span>{t("Email de tu cuenta de Atlassian")}</span>
                  <input className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required />
                </label>
              )}
              <label className="field">
                <span>{mode === "cloud" ? "API token" : t("Token de acceso personal")} {current?.site === site.trim() && t("(vacío = mantener el guardado)")}</span>
                <input className="input mono" type="password" value={token} onChange={(e) => setToken(e.target.value)}
                  required={current?.site !== site.trim()} autoComplete="off" />
              </label>
              <p className="muted" style={{ margin: 0, fontSize: 12 }}>
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
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn btn-p" disabled={busy}>{busy ? t("Comprobando…") : t("Conectar")}</button>
            {onCancel && <button type="button" className="btn" onClick={onCancel}>{t("Cancelar")}</button>}
          </div>
        </form>
      </div>
    </>
  );
}
