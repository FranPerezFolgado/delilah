import { lookFor, roleLook } from "./art/outfit";
import { recolour } from "./art/ramp";
import { DeskArt, SceneArt, Backdrop, feetStyle, seatedProps } from "./FloorScene";
import { POD_COLS, floorSize, podCount, seatPosition } from "./floorLayout";
import { PixelSprite } from "./PixelSprite";
import type { DeskSeat, FloorModel } from "./useOffice";
import { phaseLabel } from "../phases";
import { t, tn } from "../i18n";
import "./FloorMini.css";

export interface FloorMiniProps {
  floor: FloorModel;
  onOpen(owner: string): void;
  selected?: boolean;
}

/** The miniature's scale: 1 art px = 1 css px, so six floors fit a 900 px window. */
const MINI_SCALE = 1;
/** Seats of the first pod row; the miniature shows no further rows. */
const FIRST_ROW_SEATS = POD_COLS * 4;

/** One seated character in the miniature: static art, the same as the full floor's. */
function MiniDesk({ seat, project }: { seat: DeskSeat; project: string | null }) {
  const look = seat.session ? lookFor(project ?? "", seat.session) : roleLook(seat.lookKey);
  return (
    <DeskArt seat={seat} scale={MINI_SCALE}>
      <span className="fl-char fl-char--helper" style={feetStyle(seatPosition(seat.slot), MINI_SCALE)}>
        <PixelSprite {...(seatedProps(seat) as { sprite: string })} scale={MINI_SCALE} recolour={recolour(look)} />
      </span>
    </DeskArt>
  );
}

/** A floor of the building as one button: a miniature of its office floor (wall, desks with the
 *  agents behind them, lounge) at its natural size on the left; key, title, phase and pill on the right. */
export function FloorMini({ floor, onOpen, selected }: FloorMiniProps) {
  const seats = [...floor.leads, ...floor.helpers];
  const shown = seats.filter((s) => s.slot < FIRST_ROW_SEATS);
  const more = seats.length - shown.length;
  const pods = podCount(Math.max(-1, ...shown.map((s) => s.slot)) + 1);
  const size = floorSize(pods);
  const isTask = !floor.owner.startsWith("@");
  const key = isTask ? floor.owner : floor.title;
  const phase = floor.phase ? phaseLabel(floor.phase) : null;
  const agents = tn(seats.length, "{n} agente", "{n} agentes");
  const waiting = floor.waiting > 0 ? t("{n} esperando", { n: floor.waiting }) : null;
  const label = [key, phase, agents, waiting].filter(Boolean).join(", ");
  const cls = ["fm", floor.waiting > 0 && "fm-wait", selected && "fm-selected"].filter(Boolean).join(" ");
  return (
    <button type="button" className={cls} aria-label={label} title={floor.title} aria-pressed={selected} onClick={() => onOpen(floor.owner)}>
      <span className="fm-scene" aria-hidden="true" style={{ width: size.width * MINI_SCALE, height: size.height * MINI_SCALE }}>
        <Backdrop scale={MINI_SCALE} />
        <SceneArt scale={MINI_SCALE} pods={pods} />
        {shown.map((s) => (
          <MiniDesk key={s.session ?? s.helperId} seat={s} project={floor.project} />
        ))}
        {more > 0 && <span className="fm-more">+{more}</span>}
      </span>
      <span className="fm-text">
        <span className="fm-key">{key}</span>
        {isTask && <span className="fm-title">{floor.title}</span>}
        {phase && <span className="fm-phase">{phase}</span>}
        {waiting && <span className="pill c-wait fm-pill">{waiting}</span>}
      </span>
    </button>
  );
}
