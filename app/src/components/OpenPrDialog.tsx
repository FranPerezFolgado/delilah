import { useEffect, useState } from "react";
import { api, type PrProposal, type PullRequest } from "../api";
import { t } from "../i18n";
import { Button, Dialog } from "./ui";
import "./PrView.css";
import "./OpenPrDialog.css";

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

  // Loads the proposal and fills the fields. The daemon's default already points at a free
  // repo (for a parent, the first submodule with commits); `pick` loads another one.
  useEffect(() => {
    let live = true;
    api
      .prProposal(owner)
      .then((p) => {
        if (!live) return;
        setProposal(p);
        setTitle(p.title);
        setBody(p.body);
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
    <Dialog
      title={t("Abrir pull request")}
      onClose={() => !busy && onClose()}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={busy}>{t("Cancelar")}</Button>
          <Button variant="primary" disabled={busy || !proposal || !title.trim()} onClick={submit}>
            {busy ? t("Abriendo…") : t("Abrir PR")}
          </Button>
        </>
      }
    >
      <div className="open-pr">
        {proposal && (
          <div className="muted open-pr-base">
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
        <label className="open-pr-draft">
          <input type="checkbox" checked={draft} onChange={(e) => setDraft(e.target.checked)} />
          <span>{t("Borrador")}</span>
        </label>
        {error && <div className="error selectable">{error}</div>}
      </div>
    </Dialog>
  );
}
