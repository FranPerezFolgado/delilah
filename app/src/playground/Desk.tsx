import { t } from "../i18n";
import { phaseLabel } from "../phases";
import { memo } from "react";
import { accessibleName, caption, deskState, type DeskModel } from "./layout";
import { POSE_OF } from "./poses";
import { Sprite } from "./Sprite";
import { DESK, POSES } from "./sprites";

/** One desk: the monitor, the agent at it and the label with the task and its phase. */
export const Desk = memo(function Desk({ desk, onOpen }: { desk: DeskModel; onOpen: (task: string, session: string) => void }) {
  const state = deskState(desk.session, desk.activity);
  const pose = POSE_OF[state];
  const line = state === "waiting" || state === "paused" ? null : state === "supervising" ? t("Supervisando") : caption(desk.activity?.action);
  return (
    <div className={`pg-desk${state === "waiting" ? " pg-desk--waiting" : ""}`} data-owner={desk.owner} data-slot={desk.slot}>
      <div className="pg-scene">
        <Sprite className="pg-desk-art" frames={DESK} />
        <button
          type="button"
          className="pg-char"
          data-pose={pose}
          data-slot={desk.slot}
          aria-label={accessibleName(desk, desk.activity)}
          onClick={() => onOpen(desk.owner, desk.session.id)}
        >
          <Sprite frames={POSES[pose]} />
        </button>
      </div>
      {line && (
        <div className="pg-caption trunc" title={line}>
          {line}
        </div>
      )}
      <div className="pg-label">
        <span className="key">{desk.key}</span>
        <span className="trunc pg-title" title={desk.title}>
          {desk.title}
        </span>
        {desk.phase && <span className="pg-phase">{phaseLabel(desk.phase)}</span>}
      </div>
    </div>
  );
});
