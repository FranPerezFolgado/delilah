import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type JiraConnect, type JiraIssue, type JiraMode, type JiraSettings } from "../api";
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

/** Tickets from Jira, and "Analizar" to start a task on one. */
export function JiraView({ onAnalyze, onOpenTask }: Props) {
  const [settings, setSettings] = useState<JiraSettings | null | undefined>(undefined);
  const [editing, setEditing] = useState(false);
  const [issues, setIssues] = useState<JiraIssue[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const rows = await api.jiraIssues();
      setIssues(rows);
      setSelected((s) => s ?? rows[0]?.key ?? null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    api.jira().then((r) => setSettings(r.settings)).catch((e) => setError(e.message));
  }, []);
  useEffect(() => {
    if (settings) load();
  }, [settings, load]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? issues.filter((i) => `${i.key} ${i.summary} ${i.components.join(" ")}`.toLowerCase().includes(q)) : issues;
  }, [issues, filter]);
  const issue = issues.find((i) => i.key === selected);

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
      {error && <div className="error selectable" style={{ padding: "8px 20px" }}>{error}</div>}
      <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
        <div style={{ flex: 1, minWidth: 0, overflowY: "auto" }}>
          <table className="list">
            <thead>
              <tr><th>{t("Clave")}</th><th>{t("Título")}</th><th>{t("Tipo")}</th><th>{t("Prioridad")}</th><th>{t("Estado")}</th><th>Delilah</th></tr>
            </thead>
            <tbody>
              {shown.map((i) => (
                <tr key={i.key} onClick={() => setSelected(i.key)}
                  style={{ cursor: "pointer", background: i.key === selected ? "var(--sel)" : undefined,
                    boxShadow: i.key === selected ? "inset 2px 0 0 var(--acc)" : undefined }}>
                  <td className="mono" style={{ color: "var(--tx2)" }}>{i.key}</td>
                  <td className="trunc" style={{ maxWidth: 380 }}>{i.summary}</td>
                  <td className="muted">{i.issue_type}</td>
                  <td className="muted">{i.priority ?? "—"}</td>
                  <td className="muted">{i.status}</td>
                  <td>{i.task_status ? <span className="pill">{TASK_LABELS[i.task_status] ? t(TASK_LABELS[i.task_status]) : i.task_status}</span> : <span className="muted">—</span>}</td>
                </tr>
              ))}
              {shown.length === 0 && !loading && (
                <tr><td colSpan={6} className="muted">{filter ? t("No hay tickets con ese filtro.") : t("No hay tickets para esta consulta.")}</td></tr>
              )}
            </tbody>
          </table>
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
              <span className="muted">{t("Estado")}</span><span>{issue.status}</span>
              <span className="muted">{t("Asignado")}</span><span>{issue.assignee ?? t("Sin asignar")}</span>
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
              {issue.description ? <Markdown text={issue.description} /> : <span className="muted">{t("Sin descripción.")}</span>}
            </div>
          </aside>
        )}
      </div>
    </>
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
