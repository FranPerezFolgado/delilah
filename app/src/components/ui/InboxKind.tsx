import type { InboxKind as Kind } from "../../api";
import { kindLabel, kindOf } from "../../vocab";
import { Icon, type IconName } from "./Icon";
import "./InboxKind.css";

const ICON: Record<"Pregunta" | "Permiso" | "Aviso", IconName> = { Pregunta: "wait", Permiso: "lock", Aviso: "inbox" };

export function InboxKind({ kind }: { kind: Kind }) {
  const k = kindOf(kind).kind;
  return (
    <span className="kind">
      <Icon name={ICON[k]} />
      {kindLabel(k)}
    </span>
  );
}
