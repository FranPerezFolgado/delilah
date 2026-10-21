import { useCallback, useEffect, useState } from "react";
import { ago, api, type PullRequest, type TaskPrs } from "../api";
import { on } from "../live";
import { openExternal } from "../platform";
import { PR_CI, PR_STATES, REVIEWS } from "./PrBadge";
import { t } from "../i18n";

// What to do about each problem the daemon can report with `gh`.
const FIXES: Record<string, string> = {
  missing: "Instala gh: brew install gh",
  "signed-out": "Ejecuta gh auth login",
  "no-access": "Tu cuenta no tiene acceso a este repositorio",
  "unsupported-remote": "El remoto de este repositorio no es de un host que Delilah sepa consultar",
  network: "Sin conexión con el host: se reintenta solo",
  "rate-limited": "Límite de peticiones alcanzado: se reintenta más tarde",
};

const OUTCOME_CLASS = { pending: "c-wait", success: "c-done", failure: "c-err", neutral: "c-mut" } as const;

/** The PR tab: the task's PRs with their checks and review, and the means to link one by URL. */
export function PrView({ owner }: { owner: string }) {
  const [data, setData] = useState<TaskPrs | null>(null);
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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

  const link = () => {
    const u = url.trim();
    if (!u) return;
    run(async () => {
      await api.linkPr(owner, u);
      setUrl("");
    });
  };

  const problem = data?.problem;
  const checked = data?.prs.map((p) => p.checked_at).filter((c): c is string => !!c).sort().pop();

  return (
    <div className="pr-view">
      <div className="pr-head">
        <span className="muted">
          {data?.account ? t("Cuenta: {account}", { account: data.account }) : t("Sin cuenta de gh")}
        </span>
        <span style={{ flex: 1 }} />
        <button className="btn" disabled={busy} onClick={() => run(() => api.refreshPrs(owner))}>
          {t("Actualizar")}
        </button>
      </div>

      {problem && (
        <div className="notice pr-problem" style={{ borderColor: "var(--err)" }}>
          <span className="c-err">●</span>
          <span>
            {problem.message} {FIXES[problem.kind] && <b>{t(FIXES[problem.kind])}</b>}
            {(problem.kind === "network" || problem.kind === "rate-limited") && checked && (
              <span className="muted"> · {t("comprobado {when}", { when: ago(checked) })}</span>
            )}
          </span>
        </div>
      )}
      {error && <div className="error">{error}</div>}

      {data && data.prs.length === 0 && <div className="empty">{t("Aún no hay PR")}</div>}
      {data?.prs.map((pr) => (
        <PrCard key={pr.id} pr={pr} disabled={busy} onUnlink={() => run(() => api.unlinkPr(owner, pr.id))} />
      ))}

      <div className="pr-link">
        <input
          className="input"
          value={url}
          placeholder="https://github.com/…/pull/42"
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && link()}
        />
        <button className="btn" disabled={busy || !url.trim()} onClick={link}>
          {t("Vincular por URL")}
        </button>
      </div>
    </div>
  );
}

function PrCard({ pr, disabled, onUnlink }: { pr: PullRequest; disabled: boolean; onUnlink: () => void }) {
  const s = pr.snapshot;
  const state = s ? PR_STATES[s.draft && s.state === "open" ? "draft" : s.state] : null;
  return (
    <section className="pr-card">
      <div className="pr-card-top">
        <a
          className="pr-title"
          href={pr.url}
          onClick={(e) => {
            e.preventDefault();
            openExternal(pr.url);
          }}
        >
          #{pr.number} {s?.title ?? pr.owner_name}
        </a>
        {state && <span className={`pr-chip pr-${s!.state}${s!.draft ? " pr-draft" : ""}`}>{t(state.label)}</span>}
        <span style={{ flex: 1 }} />
        <button className="btn btn-ghost" disabled={disabled} onClick={onUnlink}>
          {t("Desvincular")}
        </button>
      </div>
      <div className="muted pr-meta">
        {pr.repo !== "." && <span className="mono">{pr.repo}</span>}
        {s && <span>{t("hacia {base}", { base: s.base })}</span>}
        {s?.review_decision && <span>{t(REVIEWS[s.review_decision])}</span>}
        {s && <span>{t("{n} comentarios", { n: s.comments })}</span>}
        {s && <span>{t(PR_CI[s.ci])}</span>}
        {pr.checked_at && <span>{t("comprobado {when}", { when: ago(pr.checked_at) })}</span>}
      </div>
      {pr.error && <div className="c-err pr-meta">{pr.error}</div>}
      {s && s.checks.length > 0 && (
        <ul className="pr-checks">
          {s.checks.map((c) => (
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
              <span className={`muted ${OUTCOME_CLASS[c.outcome]}`}>{c.outcome}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
