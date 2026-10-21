/** What a desk's character is doing, from the session's state. */
export type DeskState =
  | "starting"
  | "thinking"
  | "waiting"
  | "paused"
  | "edit"
  | "read"
  | "run"
  | "delegate"
  | "other"
  | "supervising";

/** Character poses with a sprite; later packages add more to the union. */
export type Pose =
  | "arriving"
  | "thinking"
  | "waiting"
  | "paused"
  | "typing"
  | "reading"
  | "terminal"
  | "phone"
  | "busy";

/** The pose that shows each desk state. */
export const POSE_OF: Record<DeskState, Pose> = {
  starting: "arriving",
  thinking: "thinking",
  waiting: "waiting",
  paused: "paused",
  edit: "typing",
  read: "reading",
  run: "terminal",
  // `delegate` is folded into `supervising` / `thinking` by deskState; mapped for safety.
  delegate: "phone",
  supervising: "phone",
  other: "busy",
};
