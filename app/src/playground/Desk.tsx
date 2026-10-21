import { phaseLabel } from "../phases";
import { deskState, POSE_OF, type DeskModel } from "./layout";
import { Sprite } from "./Sprite";
import { DESK, POSES } from "./sprites";

/** One desk: the monitor, the agent at it and the label with the task and its phase. */
export function Desk({ desk, onOpen }: { desk: DeskModel; onOpen: (task: string, session: string) => void }) {
  const state = deskState(desk.session);
  return (
    <div className="pg-desk" data-owner={desk.owner} data-slot={desk.slot}>
      <div className="pg-scene">
        <Sprite className="pg-desk-art" frames={DESK} />
        <button
          type="button"
          className="pg-char"
          data-pose={state}
          data-slot={desk.slot}
          aria-label={desk.key}
          onClick={() => onOpen(desk.owner, desk.session.id)}
        >
          <Sprite frames={POSES[POSE_OF[state]]} />
        </button>
      </div>
      <div className="pg-label">
        <span className="key">{desk.key}</span>
        <span className="trunc pg-title" title={desk.title}>
          {desk.title}
        </span>
        {desk.phase && <span className="pg-phase">{phaseLabel(desk.phase)}</span>}
      </div>
    </div>
  );
}
