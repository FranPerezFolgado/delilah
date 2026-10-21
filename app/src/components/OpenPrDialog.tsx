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

  useEffect(() => {
    api
      .prProposal(owner)
      .then((p) => {
        setProposal(p);
        setTitle(p.title);
        setBody(p.body);
      })
      .catch((e) => setError((e as Error).message));
  }, [owner]);

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
