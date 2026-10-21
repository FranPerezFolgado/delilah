/** What a desk's character is doing, from the session's state. */
export type DeskState = "starting" | "thinking" | "waiting" | "paused";

/** Character poses with a sprite; later packages add more to the union. */
export type Pose = "arriving" | "thinking" | "waiting" | "paused";

/** The pose that shows each desk state. */
export const POSE_OF: Record<DeskState, Pose> = {
  starting: "arriving",
  thinking: "thinking",
  waiting: "waiting",
  paused: "paused",
};
