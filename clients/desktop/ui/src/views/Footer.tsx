import { cmd } from "../ipc/commands";
import { usePairingsStore, useActivePairing, useStatusStore, useUiStore } from "../store";
import { useFilteredEntries } from "../store/history";
import { CONNECTION } from "./connection";
import { IconButton, StatusLight } from "./fui";

export default function Footer({ activeUserId }: { activeUserId: string | undefined }) {
  const pairingCount = usePairingsStore((s) => s.pairings.length);
  const activePairing = useActivePairing();
  const status = useStatusStore((s) => (activeUserId ? s.byUser[activeUserId] : undefined));
  const light = CONNECTION[status?.state ?? "Disconnected"];
  const pending = status?.pending ?? 0;
  const filtered = useFilteredEntries();
  const selectedIndex = useUiStore((s) => s.selectedIndex);
  const selected = filtered[selectedIndex];

  // One Pairing needs no naming - the window is unambiguously its history. The
  // count stands in when nothing is active, because then the light and the list
  // are both about nothing in particular. `last_error` is not here: it belongs
  // to the degraded strip, which has room to state it.
  const readout = activeUserId
    ? pairingCount > 1
      ? activePairing?.username?.toUpperCase()
      : undefined
    : pairingCount > 0
      ? `${pairingCount} PAIRED`
      : undefined;
  return (
    <div className="fui-band flex h-[30px] shrink-0 items-center justify-between gap-2 border-t border-hairline px-3 text-chrome uppercase tracking-phrase text-text-muted">
      <span className="flex min-w-0 items-center gap-3">
        <StatusLight tone={light.tone} label={light.label} pulse={light.pulse} testId="status" />
        {pending > 0 && <span className="shrink-0 text-amber-400">{pending} PENDING</span>}
        {readout && <span className="min-w-0 truncate">{readout}</span>}
      </span>
      <span className="flex shrink-0 items-center gap-1">
        {/*
          The handoff to the reader. It is an icon rather than a keybinding
          because ADR 0002 established there is no width here for a fourth hint,
          and a binding the hint strip cannot teach is a binding nobody finds.
          It carries the selected entry so the case that justifies the reader —
          two previews that diverge past the truncation — costs one click, not a
          manual re-find in a second window.
        */}
        <IconButton
          label="History"
          title="Open this entry in the main window"
          testId="open-history"
          onClick={() => cmd.openSection("history", selected?.id).catch(() => {})}
        >
          <HistoryIcon />
        </IconButton>
        <IconButton label="Pairings" onClick={() => cmd.openSection("pairings").catch(() => {})}>
          <PairingsIcon />
        </IconButton>
        <IconButton label="Settings" onClick={() => cmd.openSection("settings").catch(() => {})}>
          <SettingsIcon />
        </IconButton>
      </span>
    </div>
  );
}

function PairingsIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="3.5" />
    </svg>
  );
}

function HistoryIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="3.5" y="4.5" width="17" height="15" />
      <path d="M3.5 9.5h17" />
      <path d="M8.5 9.5v10" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v10" />
      <path d="M7 12h10" />
    </svg>
  );
}
