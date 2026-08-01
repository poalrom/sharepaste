import { useEffect, useState } from "react";
import { cmd } from "../../ipc/commands";
import { events } from "../../ipc/events";
import { usePairingsStore, useUiStore } from "../../store";
import { CONNECTION } from "../connection";
import { IconButton, PanelMessage, StatusLight, Strip } from "../fui";
import PairingFlow from "./PairingFlow";

export default function PairingsSection() {
  const pairings = usePairingsStore((s) => s.pairings);
  const hydrate = usePairingsStore((s) => s.hydrate);
  const removeFromStore = usePairingsStore((s) => s.remove);
  const setActiveInStore = usePairingsStore((s) => s.setActive);
  const updateStatus = usePairingsStore((s) => s.updateStatus);
  const pairingFlowOpen = useUiStore((s) => s.pairingFlowOpen);
  const setPairingFlowOpen = useUiStore((s) => s.setPairingFlowOpen);
  const [confirmingUserId, setConfirmingUserId] = useState<string | undefined>();
  /** The card holding the show-code panel, if `+ DEVICE` opened one. */
  const [deviceFlowUserId, setDeviceFlowUserId] = useState<string | undefined>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const rows = await cmd.listPairings();
        if (!cancelled) hydrate(rows);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    };
    refresh();
    const subPromise = (async () => {
      const unsubs: Array<() => void> = [];
      unsubs.push(await events.onPairingAdded(refresh));
      unsubs.push(
        await events.onPairingRemoved(({ user_id }) => {
          removeFromStore(user_id);
          // Both panels are keyed by a user id that just stopped existing; left
          // set, the add box could never open again.
          setConfirmingUserId((curr) => (curr === user_id ? undefined : curr));
          setDeviceFlowUserId((curr) => (curr === user_id ? undefined : curr));
        }),
      );
      unsubs.push(
        await events.onActivePairingChanged(({ user_id }) => {
          setActiveInStore(user_id ?? undefined);
        }),
      );
      unsubs.push(
        await events.onConnectionState(({ user_id, state }) => {
          updateStatus(user_id, state);
        }),
      );
      return unsubs;
    })();
    return () => {
      cancelled = true;
      subPromise.then((unsubs) => unsubs.forEach((u) => u()));
    };
  }, [hydrate, removeFromStore, setActiveInStore, updateStatus]);

  // One flow on screen at a time: a card that takes it wins, and the add box
  // reads closed rather than sitting open behind the panel it handed over to.
  const addOpen = pairingFlowOpen && deviceFlowUserId === undefined;

  return (
    <div className="fui-scroll flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto p-3.5">
      {pairings.length === 0 ? (
        <>
          <PanelMessage
            title="NO PAIRINGS ON THIS DEVICE"
            detail="This device holds no keys. Pair it to a relay to start receiving entries."
            action={{ label: "ADD A PAIRING", onClick: () => setPairingFlowOpen(true), testId: "add-pairing" }}
          />
          {pairingFlowOpen && (
            <div className="fui-group" data-tone="active">
              <PairingFlow />
            </div>
          )}
        </>
      ) : (
        <>
          {pairings.map((p) => {
            const conn = CONNECTION[p.status];
            /*
             * A pairing that is not the Active one holds no session, so
             * `Disconnected` is its resting state and not a fault: it reads
             * STANDBY (plan §1.4). The tone is already standby for that state —
             * only the word is wrong for a pairing nobody asked to connect.
             */
            const resting = !p.is_active && p.status === "Disconnected";
            return (
              <div key={p.user_id} className="fui-group" data-tone={p.is_active ? "active" : undefined}>
                <div className="flex items-stretch">
                  <span
                    aria-hidden="true"
                    className="w-[3px] shrink-0"
                    style={{
                      background: p.is_active
                        ? "var(--cyan-500)"
                        : p.status === "AuthFailed"
                          ? "var(--alert-400)"
                          : "var(--cyan-a20)",
                    }}
                  />
                  <div className="flex min-w-0 flex-1 items-center justify-between gap-3.5 px-3 py-2.5">
                    <div className="flex min-w-0 flex-col gap-1">
                      {/*
                        The heading names the User, not this machine. `label` is
                        the Device Label chosen when this machine paired, so
                        heading the card with it made every Pairing look like an
                        account named after the local machine.
                      */}
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="truncate font-display text-sm font-medium uppercase tracking-phrase text-text-body">
                          {p.username ?? p.user_id}
                        </span>
                        {p.is_active && (
                          <span
                            data-testid={`pair-active-${p.user_id}`}
                            className="shrink-0 border border-hairline px-1.5 py-px text-chrome tracking-word text-nominal-400"
                          >
                            ACTIVE
                          </span>
                        )}
                      </div>
                      <span
                        className="truncate font-mono text-chrome tracking-phrase text-text-muted"
                        title={p.server_url}
                      >
                        {p.user_id} @ {p.relay_host}
                      </span>
                      <span className="truncate font-mono text-chrome tracking-phrase text-text-dim">
                        THIS DEVICE: {p.label}
                      </span>
                    </div>

                    <div className="flex shrink-0 items-center gap-4 text-chrome uppercase tracking-phrase text-text-muted">
                      <StatusLight
                        tone={conn.tone}
                        label={resting ? "STANDBY" : conn.label}
                        pulse={conn.pulse}
                        testId={`pair-status-${p.user_id}`}
                      />
                      {/* The one surface that shows a queue on a pairing this
                          device has switched away from; nothing else would. */}
                      {p.pending > 0 && (
                        <span
                          data-testid={`pair-pending-${p.user_id}`}
                          className="text-chrome tracking-phrase text-amber-400"
                        >
                          {p.pending} PENDING
                        </span>
                      )}
                      {!p.is_active && (
                        <button
                          type="button"
                          data-testid={`pair-use-${p.user_id}`}
                          className="fui-action"
                          data-variant="outline"
                          onClick={async () => {
                            try {
                              await cmd.setActivePairing({ user_id: p.user_id });
                            } catch (e) {
                              setError(String(e));
                            }
                          }}
                        >
                          USE
                        </button>
                      )}
                      <button
                        type="button"
                        title="Pair another device to this pairing"
                        data-testid={`pair-add-device-${p.user_id}`}
                        className="fui-action"
                        data-variant="outline"
                        onClick={() => {
                          setDeviceFlowUserId(p.user_id);
                          setPairingFlowOpen(false);
                        }}
                      >
                        + DEVICE
                      </button>
                      <IconButton
                        label={`Forget pairing ${p.username ?? p.user_id}`}
                        tone="alert"
                        testId={`pair-forget-${p.user_id}`}
                        onClick={() => setConfirmingUserId(p.user_id)}
                      >
                        <span aria-hidden="true" className="text-chrome leading-none">⌫</span>
                      </IconButton>
                    </div>
                  </div>
                </div>

                {/*
                  ADR 0002 puts cipher disclosure beside pairing — where the
                  choice to trust a relay is being made — not in footer chrome.
                  The mock's badge said AES-256-GCM; `core/crypto.rs:3` seals
                  with XChaCha20-Poly1305.
                */}
                <div className="border-t border-hairline px-3 py-1.5 font-mono text-chrome tracking-phrase text-text-dim">
                  XCHACHA20-POLY1305
                </div>

                {confirmingUserId === p.user_id && (
                  <Strip tone="alert" testId={`confirm-strip-${p.user_id}`}>
                    <span className="shrink-0 border border-alert-400 px-1.5 py-0.5 font-mono text-chrome tracking-word">
                      ALERT
                    </span>
                    {/* Names the full user-and-relay, not the heading: two
                        pairings can share a username and this is the one action
                        that cannot be undone. */}
                    <span className="my-1 flex-1 text-data normal-case tracking-phrase">
                      Erase the local key and cached history for {p.user_id} @ {p.relay_host}? The relay
                      itself is untouched.
                    </span>
                    <button
                      type="button"
                      data-testid={`cancel-forget-${p.user_id}`}
                      className="fui-action shrink-0"
                      data-variant="outline"
                      onClick={() => setConfirmingUserId(undefined)}
                    >
                      CANCEL
                    </button>
                    <button
                      type="button"
                      data-testid={`confirm-forget-${p.user_id}`}
                      className="fui-action shrink-0 bg-alert-400 text-void-1000 focus-visible:outline-alert-400 hover:brightness-110"
                      onClick={async () => {
                        try {
                          await cmd.forgetPairing({ user_id: p.user_id });
                          removeFromStore(p.user_id);
                          setConfirmingUserId(undefined);
                        } catch (e) {
                          setError(String(e));
                        }
                      }}
                    >
                      FORGET
                    </button>
                  </Strip>
                )}

                {deviceFlowUserId === p.user_id && (
                  <div className="border-t border-hairline">
                    <PairingFlow forUserId={p.user_id} onClose={() => setDeviceFlowUserId(undefined)} />
                  </div>
                )}
              </div>
            );
          })}

          <div className={`fui-group ${addOpen ? "" : "border-dashed"}`} data-tone={addOpen ? "active" : undefined}>
            <button
              type="button"
              data-testid="add-pairing-row"
              className="flex w-full items-center justify-between gap-3.5 px-3 py-2.5 text-left text-text-body transition-colors duration-fast hover:text-cyan-300"
              onClick={() => {
                // The mirror of `+ DEVICE` closing this box. Both flows listen
                // for `pair-shortcode`, so two of them open at once would show
                // one card's code twice, once under a heading naming no pairing.
                setDeviceFlowUserId(undefined);
                setPairingFlowOpen(true);
              }}
            >
              <span className="flex min-w-0 flex-col gap-1">
                <span className="font-display text-sm font-medium tracking-phrase">Add a pairing</span>
                <span className="text-data text-text-dim">
                  Claim an operator invite, or enter a pair code another device is showing.
                </span>
              </span>
              <span aria-hidden="true" className="shrink-0 font-mono text-lg leading-none">+</span>
            </button>
            {addOpen && (
              <div className="border-t border-hairline">
                <PairingFlow />
              </div>
            )}
          </div>
        </>
      )}

      {/* One error surface for the whole pane: hydration, USE and FORGET land here. */}
      {error !== undefined && (
        <Strip tone="alert" testId="pairings-error">
          <span className="normal-case">{error}</span>
        </Strip>
      )}
    </div>
  );
}
