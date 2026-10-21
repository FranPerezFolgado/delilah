import { t } from "../i18n";
import { phaseLabel } from "../phases";
import { memo, useEffect, useRef, useState } from "react";
import type { Helper } from "../api";
import { accessibleName, assignSlots, caption, deskState, helperName, helperPose, roleLook, visibleHelpers, type DeskModel } from "./layout";
import { POSE_OF } from "./poses";
import { Sprite } from "./Sprite";
import { DESK, POSES, ROLE_PROP_FRAMES } from "./sprites";

/** Helper spots per session (helper id -> spot), module-level so they survive unmounting. */
const SPOTS = new Map<string, Map<string, number>>();
const LEAVE_MS = 300;

interface Leaving {
  helper: Helper;
  spot: number;
}

/** The helpers around a desk, at stable spots; a removed one lingers while it fades out. */
function Helpers({ sessionId, helpers }: { sessionId: string; helpers: Helper[] }) {
  const { shown, rest } = visibleHelpers(helpers);
  // Idempotent: the same helpers give the same spots, so writing during render is safe.
  const spots = assignSlots(SPOTS.get(sessionId) ?? new Map(), shown.map((h) => h.id));
  if (spots.size) SPOTS.set(sessionId, spots);
  else SPOTS.delete(sessionId);

  const prev = useRef(new Map<string, Leaving>());
  const [leaving, setLeaving] = useState<Leaving[]>([]);
  const now = new Map(shown.map((h) => [h.id, { helper: h, spot: spots.get(h.id)! }]));
  useEffect(() => {
    const gone = [...prev.current.values()].filter((l) => !now.has(l.helper.id));
    prev.current = now;
    if (!gone.length) return;
    setLeaving((cur) => [...cur, ...gone]);
    const timer = setTimeout(() => setLeaving((cur) => cur.filter((l) => !gone.includes(l))), LEAVE_MS);
    return () => clearTimeout(timer);
  });

  const cell = (h: Helper, spot: number, out = false) => {
    const { look, label } = roleLook(h.role);
    const pose = helperPose(h);
    return (
      <span
        key={out ? `out-${h.id}` : h.id}
        className={`pg-helper${out ? " pg-helper--leaving" : ""}`}
        role="img"
        aria-label={helperName(h)}
        title={label}
        data-look={look}
        data-pose={pose}
        data-spot={spot}
      >
        <Sprite frames={POSES[pose]} />
        <Sprite className="pg-prop" frames={ROLE_PROP_FRAMES[look]} />
      </span>
    );
  };
  return (
    <>
      {shown.map((h) => cell(h, spots.get(h.id)!))}
      {leaving.filter((l) => !now.has(l.helper.id)).map((l) => cell(l.helper, l.spot, true))}
      {rest.length > 0 && (
        <span className="pg-helper-more" title={rest.map((h) => roleLook(h.role).label).join(", ")}>
          +{rest.length}
        </span>
      )}
    </>
  );
}

/** One desk: the monitor, the agent at it and the label with the task and its phase. */
export const Desk = memo(function Desk({ desk, onOpen }: { desk: DeskModel; onOpen: (task: string, session: string) => void }) {
  const state = deskState(desk.session, desk.activity);
  const pose = POSE_OF[state];
  const line = state === "waiting" || state === "paused" ? null : state === "supervising" ? t("Supervisando") : caption(desk.activity?.action);
  return (
    <div className={`pg-desk${state === "waiting" ? " pg-desk--waiting" : ""}`} data-owner={desk.owner} data-slot={desk.slot}>
      <div className="pg-scene">
        <Helpers sessionId={desk.session.id} helpers={desk.activity?.helpers ?? []} />
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
