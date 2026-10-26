import { OFFICE_COLOURS } from "./art/office";
import { PixelSprite } from "./PixelSprite";
import { lookFor, roleLook } from "./art/outfit";
import { recolour } from "./art/ramp";
import type { DeskSeat, FloorModel } from "./useOffice";
import { phaseLabel } from "../phases";
import { t, tn } from "../i18n";
import "./FloorMini.css";

export interface FloorMiniProps {
  floor: FloorModel;
  onOpen(owner: string): void;
  selected?: boolean;
}

const MAX_MINIS = 6;

function Mini({ seat, project }: { seat: DeskSeat; project: string | null }) {
  const look = seat.session ? lookFor(project ?? "", seat.session) : roleLook(seat.lookKey);
  const props =
    seat.state === "paused"
      ? { animation: "seated_sleeping" }
      : seat.state === "working" && seat.action
        ? { animation: "typing" }
        : { animation: "seated" };
  return (
    <span className={`fm-mini fm-${seat.state}`}>
      <PixelSprite {...props} scale={2} recolour={recolour(look)} />
      {seat.state === "waiting" && <b className="fm-q">?</b>}
    </span>
  );
}

/** A floor of the building as one button: key, title, phase and its first characters. */
export function FloorMini({ floor, onOpen, selected }: FloorMiniProps) {
  const seats = [...floor.leads, ...floor.helpers];
  const shown = seats.slice(0, MAX_MINIS);
  const more = seats.length - shown.length;
  const isTask = !floor.owner.startsWith("@");
  const key = isTask ? floor.owner : floor.title;
  const phase = floor.phase ? phaseLabel(floor.phase) : null;
  const agents = tn(seats.length, "{n} agente", "{n} agentes");
  const waiting = floor.waiting > 0 ? t("{n} esperando", { n: floor.waiting }) : null;
  const label = [key, phase, agents, waiting].filter(Boolean).join(", ");
  const cls = ["fm", floor.waiting > 0 && "fm-wait", selected && "fm-selected"].filter(Boolean).join(" ");
  return (
    <button type="button" className={cls} aria-label={label} title={floor.title} aria-pressed={selected} onClick={() => onOpen(floor.owner)}>
      <span className="fm-head">
        <span className="fm-key">{key}</span>
        {isTask && <span className="fm-title">{floor.title}</span>}
        {phase && <span className="fm-phase">{phase}</span>}
        {waiting && <span className="pill c-wait fm-pill">{waiting}</span>}
      </span>
      <span className="fm-strip" aria-hidden="true">
        <svg className="fm-bg" width="100%" height="100%" focusable="false" shapeRendering="crispEdges">
          <rect width="100%" height="100%" fill={OFFICE_COLOURS.floor} />
          <rect width="100%" height="34%" fill={OFFICE_COLOURS.wall} />
          <rect y="30%" width="100%" height="4%" fill={OFFICE_COLOURS.wallTrim} />
        </svg>
        {shown.map((s) => (
          <Mini key={s.session ?? s.helperId} seat={s} project={floor.project} />
        ))}
        {more > 0 && <span className="fm-more">+{more}</span>}
      </span>
    </button>
  );
}
