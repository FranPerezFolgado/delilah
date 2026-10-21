import { useEffect, useState } from "react";
import { api, type PrProposal, type PullRequest } from "../api";
import { t } from "../i18n";

interface Props {
  owner: string;
  onClose: () => void;
  onOpened: (pr: PullRequest) => void;
}

/** Opens a PR for the task's branch: starts from the daemon's proposal (title and body
 *  from the branch's commits), editable, with an optional draft. */
export function OpenPrDialog({ owner, onClose, onOpened }: Props) {
  const [proposal, setProposal] = useState<PrProposal | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [draft, setDraft] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Loads the proposal for a repo and fills the fields. A parent project has no "." repo of its
  // own: the first answer then lists the submodules without a PR, and we load the first of them.
  useEffect(() => {
    let live = true;
    const fill = (p: PrProposal) => {
      setProposal(p);
      setTitle(p.title);
      setBody(p.body);
    };
    api
      .prProposal(owner)
      .then((p) => {
        if (!live) return;
        if (p.repos.length > 0 && !p.repos.includes(p.repo)) {
          return api.prProposal(owner, p.repos[0]).then((q) => live && fill({ ...q, repos: p.repos }));
        }
        fill(p);
      })
      .catch((e) => live && setError((e as Error).message));
    return () => {
      live = false;
    };
  }, [owner]);

  const pick = (repo: string) => {
    if (!proposal) return;
    setError(null);
    api
      .prProposal(owner, repo)
      .then((p) => {
        setProposal({ ...p, repos: proposal.repos });
        setTitle(p.title);
        setBody(p.body);
      })
      .catch((e) => setError((e as Error).message));
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  const submit = async () => {
    if (!proposal) return;
    setBusy(true);
    setError(null);
    try {
      const pr = await api.openPr(owner, { repo: proposal.repo, title: title.trim(), body, draft });
      onOpened(pr);
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <div className="dialog" role="dialog" aria-label={t("Abrir pull request")}>
        <h2>{t("Abrir pull request")}</h2>
        {proposal && (
          <div className="muted" style={{ fontSize: 12.5 }}>
            {t("contra {base}", { base: proposal.base })}
            {proposal.needs_push && <> · {t("Se subirá la rama antes de abrirla")}</>}
          </div>
        )}
        {proposal && (proposal.repos.length > 1 || !proposal.repos.includes(".")) && (
          <label className="field">
            <span>{t("Submódulo")}</span>
            <select className="input" value={proposal.repo} onChange={(e) => pick(e.target.value)} disabled={busy}>
              {proposal.repos.map((r) => (
                <option key={r} value={r}>{r}</option>
              ))}
            </select>
          </label>
        )}
        <label className="field">
          <span>{t("Título")}</span>
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} disabled={!proposal} autoFocus />
        </label>
        <label className="field">
          <span>{t("Descripción")}</span>
          <textarea className="input pr-open-body" value={body} onChange={(e) => setBody(e.target.value)} disabled={!proposal} />
        </label>
        <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input type="checkbox" checked={draft} onChange={(e) => setDraft(e.target.checked)} />
          <span>{t("Borrador")}</span>
        </label>
        {error && <div className="error selectable">{error}</div>}
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          <button type="button" className="btn" onClick={onClose} disabled={busy}>{t("Cancelar")}</button>
          <button type="button" className="btn btn-p" disabled={busy || !proposal || !title.trim()} onClick={submit}>
            {busy ? t("Abriendo…") : t("Abrir PR")}
          </button>
        </div>
      </div>
    </div>
  );
}
