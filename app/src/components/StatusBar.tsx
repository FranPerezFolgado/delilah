import type { UsageReport } from "../api";

const NAMES: Record<string, string> = { five_hour: "5 h", seven_day: "Semana" };

export function StatusBar({ online, active, usage }: { online: boolean; active: number; usage: UsageReport | null }) {
  return (
    <footer className="statusbar">
      <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span className="dot" style={{ background: online ? "var(--done)" : "var(--err)" }} />
        {online ? "delilahd conectado" : "delilahd sin conexión"}
      </span>
      <span>
        Agentes trabajando <span className="mono" style={{ color: "var(--tx2)" }}>{active}</span>
      </span>
      <span style={{ marginLeft: "auto", display: "flex", gap: 14 }}>
        {usage ? (
          Object.entries(usage.rate_limits).map(([name, w]) => {
            const used = Math.round(w.used_percentage ?? 0);
            const color = used >= 80 ? "var(--acc)" : "var(--tx3)";
            const reset = w.resets_at
              ? new Date(w.resets_at * 1000).toLocaleString("es-ES", { weekday: "short", hour: "2-digit", minute: "2-digit" })
              : null;
            return (
              <span key={name} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                Claude {NAMES[name] ?? name}
                <span className="bar" role="img" aria-label={`${used}% de uso`}>
                  <span style={{ width: `${Math.min(used, 100)}%`, background: color }} />
                </span>
                <span className="mono" style={{ color }}>{used}%</span>
                {reset && <span>reset {reset}</span>}
              </span>
            );
          })
        ) : (
          <span>Uso de Claude: llega con el primer turno de un agente</span>
        )}
      </span>
    </footer>
  );
}
