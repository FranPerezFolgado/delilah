import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type AskQuestion, type PermissionAsk, type PermissionDecision } from "../api";
import { t } from "../i18n";
import { Markdown } from "./Markdown";
import { ProposalCard } from "./ProposalCard";
import { Button, IconButton } from "./ui";
import "./AskCard.css";

export type AskVariant = "inbox" | "inline" | "chat";

/** How a permission card ended: the daemon recorded a decision, or it expired. */
export interface PermissionEnded {
  outcome: "answered" | "expired" | "withdrawn";
  decision?: PermissionDecision;
}

export interface AskCardProps {
  /** Pregunta: a question with options, or a task proposal. */
  ask?: { id: string; questions: AskQuestion[]; proposal?: { title: string; brief: string } | null };
  /** Permiso: the command to approve; `ended` turns it into a read-only record. */
  permission?: { sessionId: string; ask: PermissionAsk; ended?: PermissionEnded };
  /** Aviso: its text, its action and «Quitar» (only when it is not blocking). */
  notice?: { text: string; action?: { label: string; onClick: () => void }; onDismiss?: () => void };
  /** Where it sits: the Bandeja detail, the Tarea tab or the chat transcript. */
  variant?: AskVariant;
  /** «1 de N» when a task has several pending. */
  nav?: { index: number; total: number; onChange: (index: number) => void };
  /** Listens to 1–3 and ⌫ outside text fields, and says so in a hint line. */
  keys?: boolean;
  /** Opens the task a proposal created. */
  onOpenTask?: (key: string) => void;
  /** Called once answered or sent back to the terminal. */
  onDone?: () => void;
}

const isTextTarget = (el: EventTarget | null) => {
  const e = el as HTMLElement | null;
  return !!e && (/^(INPUT|TEXTAREA|SELECT)$/.test(e.tagName) || e.isContentEditable);
};

/** 1–3 pick the nth action and ⌫ removes, never while typing in a field. */
function useCardKeys(enabled: boolean | undefined, pick: (n: number) => void, remove?: () => void) {
  const latest = useRef({ pick, remove });
  latest.current = { pick, remove };
  useEffect(() => {
    if (!enabled) return;
    const on = (e: KeyboardEvent) => {
      if (isTextTarget(e.target) || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key >= "1" && e.key <= "3") latest.current.pick(Number(e.key) - 1);
      else if (e.key === "Backspace" && latest.current.remove) latest.current.remove();
    };
    document.addEventListener("keydown", on);
    return () => document.removeEventListener("keydown", on);
  }, [enabled]);
}

/** One card for everything that waits on the user, by kind: Pregunta, Permiso, Aviso.
 *  The Bandeja, the Tarea tab and the chat all use it. */
export function AskCard(props: AskCardProps) {
  const { ask, permission, notice, variant = "inline", nav, keys, onDone, onOpenTask } = props;
  if (ask?.proposal) return <ProposalCard ask={{ ...ask, proposal: ask.proposal }} onDone={onDone} onOpenTask={onOpenTask} />;
  const hint = keys && !permission?.ended ? <p className="ac-hint">{t("Teclas 1–3 y ⌫: solo funcionan fuera de los campos de texto.")}</p> : null;
  return (
    <div className={`ac ac-${variant} ask`}>
      {nav && nav.total > 1 && (
        <div className="ac-nav">
          <IconButton label={t("Anterior")} icon="back" disabled={nav.index <= 0} onClick={() => nav.onChange(nav.index - 1)} />
          <span>{t("{n} de {total}", { n: nav.index + 1, total: nav.total })}</span>
          <IconButton
            label={t("Siguiente")}
            icon="back"
            className="ac-next"
            disabled={nav.index >= nav.total - 1}
            onClick={() => nav.onChange(nav.index + 1)}
          />
        </div>
      )}
      {ask ? (
        <QuestionBody ask={ask} keys={keys} onDone={onDone} />
      ) : permission ? (
        <PermissionBody sessionId={permission.sessionId} ask={permission.ask} ended={permission.ended} keys={keys} onDone={onDone} />
      ) : notice ? (
        <NoticeBody notice={notice} keys={keys} />
      ) : null}
      {hint}
    </div>
  );
}

/** Runs an async action with a busy flag and an inline error. */
function useAct(onDone?: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const act = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      onDone?.();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, act };
}

const Err = ({ error }: { error: string | null }): ReactNode =>
  error && (
    <div className="ac-err selectable" role="alert">
      {error}
    </div>
  );

function QuestionBody({ ask, keys, onDone }: { ask: NonNullable<AskCardProps["ask"]>; keys?: boolean; onDone?: () => void }) {
  const [picked, setPicked] = useState<Record<string, string[]>>({});
  const [own, setOwn] = useState<Record<string, string>>({});
  const { busy, error, act } = useAct(onDone);
  const instant = ask.questions.length === 1 && !ask.questions[0].multiSelect;

  const answerOf = (q: AskQuestion) => own[q.question]?.trim() || (picked[q.question] ?? []).join(", ");
  const complete = ask.questions.every((q) => answerOf(q));
  const send = (answers: Record<string, string>) =>
    act(async () => {
      const res = await api.answerAsk(ask.id, answers);
      if (res?.error) throw new Error(res.error);
    });
  const sendAll = () => send(Object.fromEntries(ask.questions.map((q) => [q.question, answerOf(q)])));

  const choose = (q: AskQuestion, label: string) => {
    if (busy) return;
    if (instant) return void send({ [q.question]: label });
    setPicked((p) => {
      const now = p[q.question] ?? [];
      const next = q.multiSelect ? (now.includes(label) ? now.filter((l) => l !== label) : [...now, label]) : [label];
      return { ...p, [q.question]: next };
    });
  };

  useCardKeys(keys, (n) => {
    const q = ask.questions[0];
    const o = q?.options[n];
    if (q && o) choose(q, o.label);
  });

  return (
    <div
      className="ac-body"
      onKeyDown={(e) => {
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && complete && !busy) sendAll();
      }}
    >
      {ask.questions.map((q, qi) => {
        const shown = (picked[q.question] ?? [])[0] ?? q.options[0]?.label;
        const preview = q.options.find((o) => o.label === shown)?.preview;
        return (
          <fieldset key={q.question} className="ac-q" disabled={busy}>
            <legend>
              {q.header && <span className="ac-tag">{q.header}</span>}
              <span className="selectable">{q.question}</span>
              {q.multiSelect && <span className="ac-muted"> · {t("varias")}</span>}
            </legend>
            <div className="ac-options">
              {q.options.map((o, i) => {
                const on = (picked[q.question] ?? []).includes(o.label);
                return (
                  <button key={o.label} type="button" className={`ac-opt${on ? " on" : ""}`} onClick={() => choose(q, o.label)}>
                    {qi === 0 && i < 3 && <kbd className="ac-num">{i + 1}</kbd>}
                    <span className="ac-opt-text">
                      <b>{o.label}</b>
                      {o.description && <span className="ac-muted">{o.description}</span>}
                    </span>
                  </button>
                );
              })}
            </div>
            {preview && (
              <div className="ac-preview">
                <Markdown text={preview} remoteImages="click" />
              </div>
            )}
            <input
              className="ac-reply"
              placeholder={t("O escribe tu propia respuesta…")}
              aria-label={t("Tu respuesta a: {question}", { question: q.question })}
              value={own[q.question] ?? ""}
              onChange={(e) => setOwn((o) => ({ ...o, [q.question]: e.target.value }))}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.metaKey && !e.ctrlKey && instant && e.currentTarget.value.trim()) send({ [q.question]: e.currentTarget.value.trim() });
              }}
            />
          </fieldset>
        );
      })}
      <div className="ac-actions">
        <Button variant="primary" disabled={busy || !complete} onClick={sendAll}>
          {t("Enviar")} <span className="ac-key">⌘↵</span>
        </Button>
        <Button variant="ghost" disabled={busy} onClick={() => act(() => api.releaseAsk(ask.id))}>
          {t("Responder en la terminal")}
        </Button>
      </div>
      <Err error={error} />
    </div>
  );
}

function endedText(e: PermissionEnded): string {
  if (e.outcome === "expired") return t("Caducada · se respondió en la terminal");
  if (e.outcome === "withdrawn") return t("Retirada");
  if (e.decision === "allow_always") return t("Permitida siempre en esta tarea");
  if (e.decision === "allow_once") return t("Permitida una vez");
  return t("Denegada");
}

/** The command to approve, in mono. "Always" only exists when the daemon has an exact rule
 *  to remember, and the rule is shown next to it. There is no free text. */
function PermissionBody(p: { sessionId: string; ask: PermissionAsk; ended?: PermissionEnded; keys?: boolean; onDone?: () => void }) {
  const { busy, error, act } = useAct(p.onDone);
  const answer = (d: PermissionDecision) => act(() => api.answerPermission(p.sessionId, p.ask.id, d, d === "deny" ? "" : undefined));
  const decisions: PermissionDecision[] = p.ask.rule ? ["allow_once", "allow_always", "deny"] : ["allow_once", "deny"];
  useCardKeys(p.keys && !p.ended, (n) => decisions[n] && !busy && answer(decisions[n]));

  return (
    <div className="ac-body">
      <div className="ac-what selectable">
        <span className="ac-tag">{p.ask.tool}</span>
        <pre className="ac-cmd">{p.ask.summary}</pre>
      </div>
      {p.ended ? (
        <p className="ac-ended">{endedText(p.ended)}</p>
      ) : (
        <div className="ac-actions">
          <Button variant="secondary" disabled={busy} onClick={() => answer("allow_once")}>
            {t("Permitir una vez")}
          </Button>
          {p.ask.rule && (
            <>
              <Button variant="secondary" disabled={busy} onClick={() => answer("allow_always")}>
                {t("Permitir siempre en esta tarea")}
              </Button>
              <code className="ac-rule selectable">{p.ask.rule}</code>
            </>
          )}
          <Button variant="danger" disabled={busy} onClick={() => answer("deny")}>
            {t("Denegar")}
          </Button>
        </div>
      )}
      <Err error={error} />
    </div>
  );
}

function NoticeBody({ notice, keys }: { notice: NonNullable<AskCardProps["notice"]>; keys?: boolean }) {
  useCardKeys(keys, () => {}, notice.onDismiss);
  return (
    <div className="ac-body">
      <p className="ac-text selectable">{notice.text}</p>
      {(notice.action || notice.onDismiss) && (
        <div className="ac-actions">
          {notice.action && (
            <Button variant="secondary" onClick={notice.action.onClick}>
              {notice.action.label}
            </Button>
          )}
          {notice.onDismiss && (
            <Button variant="ghost" onClick={notice.onDismiss}>
              {t("Quitar")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
