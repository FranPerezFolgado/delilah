import { phaseLabel } from "../phases";
import { usePlayground } from "./usePlayground";

interface Props {
  onOpen: (task: string, session: string) => void;
  /** The app's connection state, when it already holds the change feed. */
  online?: boolean;
}

/** Placeholder: areas and desks as text, until the scene exists. */
export function PlaygroundView({ onOpen, online }: Props) {
  const { areas } = usePlayground(online);
  return (
    <div>
      {areas.map((a) => (
        <section key={a.project}>
          <h3>{a.project}</h3>
          {a.desks.map((d) => (
            <button key={d.session.id} className="btn btn-ghost" onClick={() => onOpen(d.owner, d.session.id)}>
              {d.key} — {d.title}
              {d.phase ? ` — ${phaseLabel(d.phase)}` : ""}
            </button>
          ))}
        </section>
      ))}
    </div>
  );
}
