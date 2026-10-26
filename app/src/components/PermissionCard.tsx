import type { PermissionAsk } from "../api";
import { AskCard, type AskCardProps } from "./AskCard";

interface Props extends Pick<AskCardProps, "onDone" | "variant" | "keys" | "nav"> {
  sessionId: string;
  ask: PermissionAsk;
}

/** A permission the agent asks for: a thin wrapper over the shared AskCard (removed with
 *  the old screens, T096). */
export function PermissionCard({ sessionId, ask, ...rest }: Props) {
  return <AskCard permission={{ sessionId, ask }} {...rest} />;
}
