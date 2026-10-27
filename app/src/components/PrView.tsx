import { useCallback, useEffect, useState } from "react";
import { api, ApiError, taskKind, type BaseStatus, type MergeMethod, type PrAutofix, type PrCheck, type PullRequest, type Task, type TaskPrs } from "../api";
import { on } from "../live";
import { openExternal } from "../platform";
import { PR_CI, REVIEWS } from "./PrBadge";
import { t } from "../i18n";
import { relativeTime } from "../time";
import { AutofixControl } from "./AutofixControl";
import { OpenPrDialog } from "./OpenPrDialog";
import { Button, ConfirmDialog, PrChip, pushToast } from "./ui";
import "./PrView.css";

// What to do about each problem the daemon can report with `gh`.
const FIXES: Record<string, string> = {
  missing: "Instala gh: brew install gh",
  "signed-out": "Ejecuta gh auth login",
  "no-access": "Tu cuenta no tiene acceso a este repositorio",
  "unsupported-remote": "El remoto de este repositorio no es de un host que Delilah sepa consultar",
  network: "Sin conexión con el host: se reintenta solo",
  "rate-limited": "Límite de peticiones alcanzado: se reintenta más tarde",
};

// The one-line headline of each problem; the daemon's own message is only shown for kinds we don't know.
const HEADLINES: Record<string, string> = {
  missing: "gh no está instalado",
  "signed-out": "gh no tiene una sesión iniciada",
  "no-access": "Sin acceso a este repositorio",
  "unsupported-remote": "No se pueden consultar las PR de este remoto",
  network: "Sin conexión con el host",
  "rate-limited": "Límite de peticiones alcanzado",
};

const BLOCKING = new Set(["missing", "signed-out", "no-access", "unsupported-remote"]);

const METHODS: Record<MergeMethod, string> = {
  merge: "merge commit",
  squash: "squash",
  rebase: "rebase",
};

const NO_METHODS = "No se pudo saber qué métodos de fusión permite el repositorio";

const CHECK_STATE: Record<PrCheck["outcome"], string> = {
  pending: "en curso",
  success: "correcto",
  failure: "falló",
  neutral: "omitido",
};

/** «1 min 12 s», «8 s». */
function duration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  return m > 0 ? t("{m} min {s} s", { m, s: total % 60 }) : t("{s} s", { s: total });
}

/** How long a check took, or has been running; null when the host gave no start time. */
function checkTime(c: PrCheck): string | null {
  if (!c.started_at) return null;
  const start = Date.parse(c.started_at);
  if (c.finished_at) return duration(Date.parse(c.finished_at) - start);
  return c.outcome === "pending" ? duration(Date.now() - start) : null;
}

// Why "Abrir PR" is disabled, by the reason the daemon gives.
const CANNOT: Record<string, string> = {
  "no-commits": "No hay commits nuevos en la rama",
  "has-pr": "Ya hay una PR abierta",
  "no-workspace": "La tarea no tiene workspace",
};

// Why autofix stopped on a PR.
const STOP_REASON: Record<string, string> = {
  limit: "alcanzó el límite de intentos",
  "no-push": "el agente no subió ningún cambio",
  "no-session": "no hay sesión del agente",
};

const OUTCOME_CLASS = { pending: "c-wait", success: "c-done", failure: "c-err", neutral: "c-mut" } as const;

/** The PR tab: the task's PRs with their checks and review, and the means to link one by URL. */
export function PrView({ owner, task, onTaskChanged, onOpenTerminal }: { owner: string; task?: Task | null; onTaskChanged?: () => void; onOpenTerminal?: () => void }) {
  const [data, setData] = useState<TaskPrs | null>(null);
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // The task's own switch: the answer to our last change until the task prop catches up.
  const [override, setOverride] = useState<boolean | null>(null);
  useEffect(() => setOverride(null), [task?.ci_autofix]);
  const autofix = task ? (override ?? !!task.ci_autofix) : null;
  // The daemon refuses autofix changes (409 `terminal`) on a done or a failed task.
  const finished = task?.status.type === "done" || task?.status.type === "failed";

  // Whether a PR can be opened: null while unknown, else the reason it can't (or "" if it can).
  const [cannot, setCannot] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  // Why the proposal failed, kept apart from its text: a parent task may open anyway on `no-commits`.
  const [cannotReason, setCannotReason] = useState<string | null>(null);
  // A merge the daemon held back because the parent task still has live subtasks.
  const [held, setHeld] = useState<{ pr: PullRequest; method: MergeMethod; names: string[] } | null>(null);
  const isParent = !!task && taskKind(task) === "parent_task";
  const [base, setBase] = useState<BaseStatus | null>(null);
  const none = !!data && data.prs.length === 0;
  // Re-asks whenever the set of PRs changes: a repo may be free even if the task has PRs.
  const prKey = data ? data.prs.map((p) => p.id).join(",") : null;

  useEffect(() => {
    if (prKey === null) return;
    let live = true;
    api
      .prProposal(owner)
      .then(() => live && (setCannot(""), setCannotReason(null)))
      .catch((e) => {
        if (!live) return;
        const reason = e instanceof ApiError ? e.reason : undefined;
        setCannotReason(reason ?? null);
        setCannot(reason ? t(CANNOT[reason] ?? e.message) : e instanceof ApiError ? e.message : "");
      });
    return () => {
      live = false;
    };
  }, [owner, prKey]);

  const load = useCallback(() => {
    api
      .taskPrs(owner)
      .then(setData)
      .catch(() => {});
  }, [owner]);

  useEffect(() => {
    load();
    return on("prs", load);
  }, [load]);

  // A parent task's branch may lack the commits of subtasks merged into the integration branch.
  const loadBase = useCallback(() => {
    if (isParent) api.baseStatus(owner).then(setBase).catch(() => {});
  }, [owner, isParent]);
  useEffect(() => {
    loadBase();
    return on("prs", loadBase);
  }, [loadBase]);

  const merge = async (pr: PullRequest, method: MergeMethod, confirmLive = false) => {
    setBusy(true);
    setError(null);
    try {
      await (confirmLive ? api.mergePr(owner, pr.id, method, true) : api.mergePr(owner, pr.id, method));
      pushToast({ text: t("PR #{n} fusionada", { n: pr.number }) });
    } catch (e) {
      if (e instanceof ApiError && e.reason === "live-subtasks") {
        const names = e.detail?.subtasks;
        setHeld({ pr, method, names: Array.isArray(names) ? names.map(String) : [] });
      } else setError((e as Error).message);
    } finally {
      // A 409 means the PR changed under us (checks turned failing, …): show where it stands now.
      load();
      setBusy(false);
    }
  };

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
      load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const setAutofixMode = async (f: () => Promise<Task>) => {
    setBusy(true);
    setError(null);
    try {
      const updated = await f();
      setOverride(!!updated.ci_autofix);
      onTaskChanged?.();
      load();
    } catch (e) {
      setError(e instanceof ApiError && e.reason === "terminal" ? t("La tarea ya terminó: el autofix no se puede cambiar") : (e as Error).message);
      // The PR may have changed under us: show where it stands now.
      load();
    } finally {
      setBusy(false);
    }
  };

  const openEmpty = () =>
    run(() => api.openPr(owner, { repo: ".", title: task?.title ?? owner, body: "", draft: true, empty_commit: true }));

  const link = () => {
    const u = url.trim();
    if (!u) return;
    run(async () => {
      await api.linkPr(owner, u);
      setUrl("");
    });
  };

  // With PRs in more than one repo, the cards go under a heading per submodule path.
  const byRepo = new Map<string, PullRequest[]>();
  for (const pr of data?.prs ?? []) byRepo.set(pr.repo, [...(byRepo.get(pr.repo) ?? []), pr]);
  const groups = [...byRepo.entries()];
  const grouped = groups.length > 1;

  const problem = data?.problem;
  // Without a usable gh session nothing can be opened, whatever the proposal says.
  const why = problem && BLOCKING.has(problem.kind) ? t(FIXES[problem.kind]) : cannot;
  const checked = data?.prs.map((p) => p.checked_at).filter((c): c is string => !!c).sort().pop();

  return (
    <div className="pr-view">
      <div className="pr-head">
        <span className="muted">
          {data?.account ? t("Cuenta: {account}", { account: data.account }) : t("Sin cuenta de gh")}
        </span>
        <span className="pr-grow" />
        {autofix !== null && (
          <AutofixControl
            on={autofix}
            busy={busy}
            disabled={finished}
            onChange={(on) => setAutofixMode(() => api.setAutofix(owner, on ? "on" : "off"))}
          />
        )}
        {!none && cannot === "" && (
          <Button variant="primary" size={28} onClick={() => setOpening(true)}>
            {t("Abrir PR")}
          </Button>
        )}
        <Button variant="secondary" size={28} icon="refresh" disabled={busy} onClick={() => run(() => api.refreshPrs(owner))}>
          {t("Recargar")}
        </Button>
      </div>

      {problem && (
        <div className="notice pr-problem" role="alert">
          <span className="c-err">●</span>
          <div className="pr-problem-body">
            <b>{HEADLINES[problem.kind] ? t(HEADLINES[problem.kind]) : problem.message}</b>
            {FIXES[problem.kind] && <div>{t(FIXES[problem.kind])}</div>}
            {(problem.kind === "network" || problem.kind === "rate-limited") && checked && (
              <span className="muted">{t("comprobado {when}", { when: relativeTime(checked) })}</span>
            )}
            <div className="pr-problem-actions">
              {onOpenTerminal && (problem.kind === "signed-out" || problem.kind === "missing") && (
                <Button variant="secondary" size={28} icon="console" onClick={onOpenTerminal}>
                  {t("Abrir una terminal")}
                </Button>
              )}
              <Button variant="secondary" size={28} icon="retry" disabled={busy} onClick={() => run(() => api.refreshPrs(owner))}>
                {t("Reintentar")}
              </Button>
            </div>
          </div>
        </div>
      )}
      {error && <div className="error">{error}</div>}
      {isParent && !!base?.subtask_commits && (
        <div className="pr-base-warning" role="status">
          {t("{n} commits de subtareas fusionados que no están en tu workspace", { n: base.subtask_commits })}
        </div>
      )}

      {none && (
        <div className="empty">
          <div>{t("Aún no hay PR")}</div>
          {cannot !== null && (
            <div className="pr-open">
              <Button variant="primary" disabled={!!why} title={why || undefined} onClick={() => setOpening(true)}>
                {t("Abrir PR")}
              </Button>
              {why && <span className="muted">{why}</span>}
              {isParent && cannotReason === "no-commits" && !problem && (
                <Button variant="secondary" disabled={busy} onClick={openEmpty}>
                  {t("Añadir un commit vacío y abrir")}
                </Button>
              )}
            </div>
          )}
        </div>
      )}
      {opening && (
        <OpenPrDialog
          owner={owner}
          onClose={() => setOpening(false)}
          onOpened={() => {
            setOpening(false);
            load();
          }}
        />
      )}
      {groups.map(([repo, prs]) => (
        <div key={repo} className="pr-group">
          {grouped && <h3 className="pr-group-title mono">{repo}</h3>}
          {prs.map((pr) => (
            <PrCard
              key={pr.id}
              pr={pr}
              disabled={busy || finished}
              grouped={grouped}
              taskAutofix={autofix}
              onRetryAutofix={() => setAutofixMode(() => api.setAutofix(owner, "retry", pr.repo))}
              onUnlink={() => run(() => api.unlinkPr(owner, pr.id))}
              onMerge={(method) => merge(pr, method)}
              readyHint={isParent && !!task?.subtask_progress?.ready}
              onReady={() => run(() => api.markReady(owner, pr.id))}
            />
          ))}
        </div>
      ))}

      {held && (
        <ConfirmDialog
          danger
          title={t("Subtareas sin terminar")}
          body={t("Estas subtareas siguen activas: {names}. Se quedarán sin integrar. ¿Fusionar igualmente?", { names: held.names.join(", ") })}
          action={t("Fusionar igualmente")}
          onClose={() => setHeld(null)}
          onConfirm={() => {
            const h = held;
            setHeld(null);
            merge(h.pr, h.method, true);
          }}
        />
      )}

      <div className="pr-link">
        <input
          className="input"
          value={url}
          placeholder="https://github.com/…/pull/42"
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && link()}
        />
        <Button variant="secondary" disabled={busy || !url.trim()} onClick={link}>
          {t("Vincular por URL")}
        </Button>
      </div>
    </div>
  );
}

/** The one-line state of autofix on a PR; no record means "watching" if the task has it on. */
function autofixText(a: PrAutofix | null | undefined, taskOn: boolean): string {
  if (!taskOn) return t("Autofix: apagado");
  if (!a) return t("Autofix: vigilando");
  if (a.state === "watching") return t("Autofix: vigilando");
  if (a.state === "fixing") return t("Arreglando CI: intento {n} de {limit}", { n: a.attempts, limit: a.limit });
  const why = a.reason ? STOP_REASON[a.reason] : undefined;
  return why ? `${t("Autofix: parado")} · ${t(why)}` : t("Autofix: parado");
}

function PrCard({
  pr,
  disabled,
  grouped,
  taskAutofix,
  onRetryAutofix,
  onUnlink,
  onMerge,
  readyHint,
  onReady,
}: {
  pr: PullRequest;
  disabled: boolean;
  grouped: boolean;
  taskAutofix: boolean | null;
  onRetryAutofix: () => void;
  onUnlink: () => void;
  onMerge: (method: MergeMethod) => void;
  /** A parent task whose subtasks are all merged: a draft PR can go ready. */
  readyHint: boolean;
  onReady: () => void;
}) {
  const s = pr.snapshot;
  const [confirming, setConfirming] = useState(false);
  const method = pr.method ?? s?.merge_methods?.[0];
  // A subtask's PR aimed somewhere other than its parent's branch (no retarget action).
  const misbased = !!pr.base_branch && !!pr.expected_base && pr.base_branch !== pr.expected_base;
  // Why "Fusionar" is unavailable, or null when it can go ahead.
  const blocked = !s
    ? null
    : !method
      ? t(NO_METHODS)
      : s.ci !== "success"
        ? s.ci === "failure"
          ? t("Hay checks fallando")
          : t("Los checks aún no han terminado correctamente")
        : null;
  const comments = s?.review_comments ?? [];
  const failedChecks = (s?.checks ?? []).filter((c) => c.outcome === "failure").map((c) => c.name);
  return (
    <section className="pr-card">
      <div className="pr-card-top">
        <span className="pr-chip">
          <PrChip pr={{ number: pr.number, snapshot: s }} />
        </span>
        <a
          className="pr-title"
          href={pr.url}
          onClick={(e) => {
            e.preventDefault();
            openExternal(pr.url);
          }}
        >
          {s?.title ?? pr.owner_name}
        </a>
        {s?.draft && s.state === "open" && <span className="pr-draft">{t("Borrador")}</span>}
        <span className="pr-grow" />
        <Button variant="ghost" size={28} disabled={disabled} onClick={onUnlink}>
          {t("Desvincular")}
        </Button>
      </div>
      <div className="muted pr-meta">
        {!grouped && pr.repo !== "." && <span className="mono">{pr.repo}</span>}
        {s && <span>{t("hacia {base}", { base: s.base })}</span>}
        {s?.review_decision && <span>{t(REVIEWS[s.review_decision])}</span>}
        {s && <span>{t("{n} comentarios", { n: s.comments })}</span>}
        {s && <span>{t(PR_CI[s.ci])}</span>}
        {pr.checked_at && <span>{t("comprobado {when}", { when: relativeTime(pr.checked_at) })}</span>}
      </div>
      {misbased && (
        <div className="pr-base-warning" role="alert">
          {t("Esta PR apunta a {base}, pero debería apuntar a {expected}", { base: pr.base_branch ?? "", expected: pr.expected_base ?? "" })}
        </div>
      )}
      {readyHint && s?.draft && s.state === "open" && (
        <div className="pr-ready-hint">
          <span>{t("Todas las subtareas están fusionadas: la PR puede salir de borrador")}</span>
          <Button variant="primary" size={28} disabled={disabled} onClick={onReady}>
            {t("Marcar como lista")}
          </Button>
        </div>
      )}
      {(pr.autofix || taskAutofix !== null) && (
        <div className="muted pr-meta pr-autofix">
          <span>{autofixText(pr.autofix, !!taskAutofix)}</span>
          {pr.autofix?.state === "fixing" && failedChecks.length > 0 && (
            <span>{t("Checks fallidos: {names}", { names: failedChecks.join(", ") })}</span>
          )}
          {pr.autofix?.state === "fixing" && pr.autofix.sent_at && (
            <span>{t("enviado {when}", { when: relativeTime(pr.autofix.sent_at) })}</span>
          )}
          {taskAutofix && pr.autofix?.state === "stopped" && (
            <Button variant="secondary" size={28} icon="retry" disabled={disabled} onClick={onRetryAutofix}>
              {t("Reintentar autofix")}
            </Button>
          )}
        </div>
      )}
      {pr.error && <div className="c-err pr-meta">{pr.error}</div>}
      {s && s.checks.length > 0 && (
        <ul className="pr-checks" role="list" aria-label={t("Checks")}>
          {s.checks.map((c) => {
            const time = checkTime(c);
            return (
              <li key={c.name}>
                <span className={`pr-dot pr-ci-${c.outcome}`} data-ci={c.outcome} />
                {c.url ? (
                  <a
                    href={c.url}
                    onClick={(e) => {
                      e.preventDefault();
                      openExternal(c.url!);
                    }}
                  >
                    {c.name}
                  </a>
                ) : (
                  <span>{c.name}</span>
                )}
                <span className={`muted ${OUTCOME_CLASS[c.outcome]}`}>{t(CHECK_STATE[c.outcome])}</span>
                {time && <span className="muted mono">{time}</span>}
              </li>
            );
          })}
        </ul>
      )}
      {comments.length > 0 && (
        <ul className="pr-comments" role="list" aria-label={t("Comentarios de revisión")}>
          {comments.map((c, i) => (
            <li key={i}>
              <div className="muted pr-comment-head">
                <b>{c.author}</b>
                {c.path && <span className="mono">{c.line ? `${c.path}:${c.line}` : c.path}</span>}
                <span>{relativeTime(c.at)}</span>
              </div>
              <div className="selectable">{c.body}</div>
            </li>
          ))}
        </ul>
      )}
      <div className="pr-actions">
        <Button variant="secondary" size={28} icon="external" onClick={() => openExternal(pr.url)}>
          {t("Abrir en GitHub")}
        </Button>
        {s?.state === "open" && (
          <>
            <Button
              variant="primary"
              size={28}
              icon="merge"
              disabled={disabled || !!blocked}
              title={blocked ?? undefined}
              onClick={() => setConfirming(true)}
            >
              {t("Fusionar")}
            </Button>
            {blocked ? (
              <span className="muted pr-merge-note">{blocked}</span>
            ) : (
              method && <span className="muted pr-merge-note">{t("Método: {method}", { method: METHODS[method] })}</span>
            )}
          </>
        )}
      </div>
      {confirming && method && (
        <ConfirmDialog
          title={t("Fusionar #{n}", { n: pr.number })}
          body={t("Se fusionará #{n} en {base} con el método {method}.", { n: pr.number, base: s!.base, method: METHODS[method] })}
          action={t("Fusionar")}
          onClose={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            onMerge(method);
          }}
        />
      )}
    </section>
  );
}
