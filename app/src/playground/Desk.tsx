import { t } from "../i18n";
import { phaseLabel } from "../phases";
import { memo, useEffect, useRef, useState } from "react";
import type { Helper } from "../api";
import { accessibleName, assignSlots, caption, deskState, helperName, helperPose, roleLook, visibleHelpers, type DeskModel } from "./layout";
import { POSE_OF } from "./poses";
import { Sprite } from "./Sprite";
import { DESK, POSES, ROLE_PROP_FRAMES, type Look } from "./sprites";

/** Helper spots per session (helper id -> spot), module-level so they survive unmounting. */
const SPOTS = new Map<string, Map<string, number>>();
const LEAVE_MS = 300;

interface Leaving {
  helper: Helper;
  spot: number;
}

/** The short text under a helper: the known looks have a name, the generic one shows its role. */
const LOOK_NAME = { researcher: "Investigador", implementer: "Implementador", reviewer: "Revisor" } as const;
const shortLabel = (look: Look, label: string) => (look === "generic" ? label : t(LOOK_NAME[look]));

/** The helpers around a desk, at stable spots; a removed one lingers while it fades out,
 *  keeping its spot until its own timer removes it. */
function Helpers({ sessionId, helpers }: { sessionId: string; helpers: Helper[] }) {
  const { shown, rest } = visibleHelpers(helpers);
  const prev = useRef(new Map<string, Leaving>());
  const departing = useRef(new Map<string, Leaving>());
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const [, bump] = useState(0);

  const shownIds = new Set(shown.map((h) => h.id));
  for (const [id, l] of prev.current) if (!shownIds.has(id) && !departing.current.has(id)) departing.current.set(id, l);
  for (const id of shownIds) departing.current.delete(id);
  // Idempotent: the same helpers give the same spots, so writing during render is safe.
  // Leaving helpers still count, so a newcomer does not take their spot.
  const spots = assignSlots(SPOTS.get(sessionId) ?? new Map(), [...shownIds, ...departing.current.keys()]);
  if (spots.size) SPOTS.set(sessionId, spots);
  else SPOTS.delete(sessionId);
  prev.current = new Map(shown.map((h) => [h.id, { helper: h, spot: spots.get(h.id)! }]));

  // One timer per departure, kept across re-renders; only its own firing or a return clears it.
  useEffect(() => {
    for (const id of departing.current.keys()) {
      if (timers.current.has(id)) continue;
      timers.current.set(
        id,
        setTimeout(() => {
          timers.current.delete(id);
          departing.current.delete(id);
          bump((n) => n + 1);
        }, LEAVE_MS),
      );
    }
    for (const [id, timer] of timers.current) {
      if (departing.current.has(id)) continue;
      clearTimeout(timer);
      timers.current.delete(id);
    }
  });
  useEffect(() => {
    const all = timers.current;
    return () => all.forEach(clearTimeout);
  }, []);

  const cell = (h: Helper, spot: number, out = false) => {
    const { look, label } = roleLook(h.role);
    const pose = helperPose(h);
    return (
      <span
        key={out ? `out-${h.id}` : h.id}
        className={`pg-helper${out ? " pg-helper--leaving" : ""}`}
        role="img"
        aria-label={helperName(h)}
        aria-hidden={out ? true : undefined}
        title={label}
        data-look={look}
        data-pose={pose}
        data-spot={spot}
      >
        <Sprite frames={POSES[pose]} />
        <Sprite className="pg-prop" frames={ROLE_PROP_FRAMES[look]} />
        <span className="pg-helper-label" title={label}>
          {shortLabel(look, label)}
        </span>
      </span>
    );
  };
  return (
    <>
      {shown.map((h) => cell(h, spots.get(h.id)!))}
      {[...departing.current.values()].map((l) => cell(l.helper, spots.get(l.helper.id) ?? l.spot, true))}
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
