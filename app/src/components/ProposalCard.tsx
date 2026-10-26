import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { t } from "../i18n";
import { Markdown } from "./Markdown";
import { Button, ConfirmDialog, dismissToast, pushToast } from "./ui";
import "./ProposalCard.css";

/** How long «Deshacer» works before the discard is sent to the daemon. */
const UNDO_MS = 6000;

interface Props {
  ask: { id: string; questions: { question: string }[]; proposal: { title: string; brief: string } };
  /** Opens the task the proposal created. */
  onOpenTask?: (key: string) => void;
  /** Called once the ask is resolved. */
  onDone?: () => void;
}

/** A task proposal from an idea conversation: title, brief and Crear / Crear con
 *  autopiloto (asks first, like the header toggle) / Cambiar algo / Descartar (with Deshacer). */
export function ProposalCard({ ask, onDone, onOpenTask }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<string | null>(null);
  // The daemon answered 200 with an error: the ask is resolved, so no more buttons.
  const [failed, setFailed] = useState(false);
  const [asking, setAsking] = useState(false);
  // Descartar hides the card and sends the answer after the undo window.
  const [discarding, setDiscarding] = useState(false);
  const pending = useRef<{ timer: ReturnType<typeof setTimeout>; toastId: number } | null>(null);
  const q = ask.questions[0]?.question ?? "";

  const send = async (answer: string, launch?: "autopilot") => {
    setBusy(true);
    setError(null);
    try {
      const res = launch ? await api.answerAsk(ask.id, { [q]: answer }, launch) : await api.answerAsk(ask.id, { [q]: answer });
      if (res?.error) {
        setError(res.error);
        setFailed(true);
      }
      if (res?.task) setCreated(res.task);
      onDone?.();
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  };

  const sendRef = useRef(send);
  sendRef.current = send;

  // Leaving with a discard pending sends it now rather than losing it.
  useEffect(
    () => () => {
      if (pending.current) {
        clearTimeout(pending.current.timer);
        dismissToast(pending.current.toastId);
        pending.current = null;
        void sendRef.current("Descartar");
      }
    },
    [],
  );

  const discard = () => {
    setDiscarding(true);
    // The toast can be paused by hover; the discard can't, so it closes the toast itself.
    const timer = setTimeout(() => {
      if (pending.current) dismissToast(pending.current.toastId);
      pending.current = null;
      void sendRef.current("Descartar");
    }, UNDO_MS);
    const toastId = pushToast({
      text: t("Tarea descartada"),
      ttl: UNDO_MS,
      action: {
        label: t("Deshacer"),
        run: () => {
          if (!pending.current) return;
          clearTimeout(pending.current.timer);
          pending.current = null;
          setDiscarding(false);
        },
      },
    });
    pending.current = { timer, toastId };
  };

  if (discarding && !error) return null;

  return (
    <div className="pc">
      <h3 className="pc-title selectable">{ask.proposal.title}</h3>
      <div className="pc-brief">
        <Markdown text={ask.proposal.brief} remoteImages="click" />
      </div>
      {created ? (
        <div className="pc-created">
          {t("Creada")}{" "}
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault();
              onOpenTask?.(created);
            }}
          >
            {created}
          </a>
        </div>
      ) : (
        !failed && (
          <div className="pc-actions">
            <Button variant="primary" disabled={busy} onClick={() => send("Crear")}>
              {t("Crear")}
            </Button>
            <Button variant="secondary" disabled={busy} onClick={() => setAsking(true)}>
              {t("Crear con autopiloto")}
            </Button>
            <Button variant="secondary" disabled={busy} onClick={() => send("Cambiar algo")}>
              {t("Cambiar algo")}
            </Button>
            <Button variant="ghost" disabled={busy} onClick={discard}>
              {t("Descartar")}
            </Button>
          </div>
        )
      )}
      {error && <div className="pc-error selectable">{error}</div>}
      {asking && (
        <ConfirmDialog
          title={t("Activar autopiloto")}
          body={t("El autopiloto trabaja solo hasta abrir las PR y gasta cuota. Ahorro se queda activado.")}
          action={t("Iniciar")}
          onClose={() => setAsking(false)}
          onConfirm={() => {
            setAsking(false);
            void send("Crear", "autopilot");
          }}
        />
      )}
    </div>
  );
}
