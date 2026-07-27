import { agePhrase, relayHost } from "../../lib/format";
import { useContactStore, useStatusStore } from "../../store";
import { useActivePairing } from "../../store/pairings";
import { CONNECTION } from "../connection";
import { StatusLight } from "../fui";

/**
 * The window's status band.
 *
 * It states **device-wide** facts, so it always describes the Active Pairing —
 * never the Viewed one. The History pane says what it is looking at; this says
 * what the machine is doing, and the two are allowed to differ.
 *
 * The mock printed `AES-256-GCM · LAST SYNC 14:22:07` here. Both were wrong:
 * the cipher is XChaCha20-Poly1305 and belongs beside pairing (ADR 0002), and
 * "sync" plus a wall-clock time are on Contact's _Avoid_ line — an absolute
 * timestamp means nothing three days later.
 */
export default function MainFooter({ now }: { now: number }) {
  const active = useActivePairing();
  const status = useStatusStore((s) => (active ? s.byUser[active.user_id] : undefined));
  const lastContactAt = useContactStore((s) =>
    active ? s.lastContactByUser[active.user_id] ?? null : null,
  );
  const conn = CONNECTION[status?.state ?? "Disconnected"];
  const pending = status?.pending ?? 0;

  return (
    <div className="fui-band flex h-[30px] shrink-0 items-center justify-between gap-3 border-t border-emitter px-3 text-chrome uppercase tracking-phrase text-text-muted">
      <span className="flex min-w-0 items-center gap-3">
        <StatusLight
          tone={active ? conn.tone : "standby"}
          label={active ? conn.label : "NO ACTIVE PAIRING"}
          pulse={active ? conn.pulse : false}
          testId="footer-status"
        />
        {pending > 0 && <span className="shrink-0 text-amber-400">{pending} PENDING</span>}
        {active && (
          <span className="min-w-0 truncate font-mono" data-testid="footer-identity">
            {`${active.username ?? active.user_id}@${relayHost(active.server_url)}`.toUpperCase()}
          </span>
        )}
      </span>
      {active && (
        <span className="shrink-0 font-mono text-text-dim" data-testid="footer-contact">
          LAST CONTACT{" "}
          {lastContactAt === null ? (
            "NEVER"
          ) : (
            <span className="normal-case">{agePhrase(lastContactAt, now)}</span>
          )}
        </span>
      )}
    </div>
  );
}
