import { useEffect, useRef, useState } from "react";
import type { Settings, UpdateAvailable } from "../../types";
import { cmd } from "../../ipc/commands";
import { events } from "../../ipc/events";
import { relayHost } from "../../lib/format";
import { usePairingsStore } from "../../store";
import { useActivePairing } from "../../store/pairings";
import { PanelMessage, Strip } from "../fui";

export default function SettingsSection() {
  const activeUserId = usePairingsStore((s) => s.active);
  const activePairing = useActivePairing();
  const hydratePairings = usePairingsStore((s) => s.hydrate);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string>();
  const [hotkeyDraft, setHotkeyDraft] = useState("");
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [version, setVersion] = useState<string>();
  const [available, setAvailable] = useState<UpdateAvailable | null>(null);
  const [checkState, setCheckState] = useState<"idle" | "asking" | "upToDate">("idle");
  const committedHotkey = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    cmd.getSettings().then((s) => {
      if (cancelled) return;
      setSettings(s);
      setHotkeyDraft(s.hotkey ?? "");
      committedHotkey.current = s.hotkey ?? null;
    }).catch((e) => {
      if (!cancelled) setError(String(e));
    });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (activeUserId) return;
    let cancelled = false;
    cmd.listPairings()
      .then((rows) => {
        if (!cancelled && !usePairingsStore.getState().active) hydratePairings(rows);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => { cancelled = true; };
  }, [activeUserId, hydratePairings]);

  // Last of the mount effects on purpose: the status read costs no request, so
  // it can sit behind the two that actually populate the pane.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    cmd.getUpdateStatus()
      .then((s) => {
        if (cancelled) return;
        setVersion(s.current_version);
        setAvailable(s.available ?? null);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    // The launch check runs before this pane exists, so its find arrives as an
    // event rather than in the status read above.
    events.onUpdateAvailable((found) => setAvailable(found)).then((off) => {
      if (cancelled) off(); else unlisten = off;
    });
    return () => { cancelled = true; unlisten?.(); };
  }, []);

  if (!settings) return <PanelMessage title="LOADING" />;

  const update = async (patch: Partial<Settings>) => {
    try {
      const next = await cmd.updateSettings(patch);
      setSettings(next);
    } catch (e) {
      setError(String(e));
    }
  };

  // The hotkey is only persisted on blur or Enter: every update_settings re-registers
  // the global shortcut, so committing per keystroke would leave it unbound while typing.
  const commitHotkey = () => {
    const hotkey = hotkeyDraft || null;
    if (hotkey === committedHotkey.current) return;
    committedHotkey.current = hotkey;
    update({ hotkey });
  };

  const clearHistory = async () => {
    if (!activeUserId) return;
    try {
      await cmd.clearHistory({ user_id: activeUserId });
      setConfirmingClear(false);
    } catch (e) {
      setError(String(e));
    }
  };

  const checkForUpdate = async () => {
    setCheckState("asking");
    try {
      const status = await cmd.checkForUpdate();
      setVersion(status.current_version);
      setAvailable(status.available ?? null);
      setCheckState(status.available ? "idle" : "upToDate");
    } catch (e) {
      setCheckState("idle");
      setError(String(e));
    }
  };

  const installUpdate = async () => {
    try {
      await cmd.installUpdate();
    } catch (e) {
      setError(String(e));
    }
  };

  // Nothing else on screen states the blur/Enter rule, so the hint has to carry it —
  // and a hint that read the same while the draft was dirty would be naming a binding
  // that is not in force. Hence three readings, one per state the field can be in.
  const committed = committedHotkey.current;
  const hotkeyDirty = (hotkeyDraft || null) !== committed;
  const hotkeyHint = hotkeyDirty
    ? `Press Enter or click away to re-bind — the shortcut stays on ${committed ?? "nothing"} until then.`
    : committed
      ? "Bound. Press this anywhere to summon the popover."
      : "Unbound. Bind a shortcut to summon the popover from anywhere.";

  const denyCount = settings.deny_list.length;

  const updateHint =
    checkState === "asking"
      ? "Asking the Update Source…"
      : available
        ? `Version ${available.version} is waiting.`
        : checkState === "upToDate"
          ? "Up to date."
          : "Ask now instead of waiting for the next launch.";

  return (
    <div className="fui-scroll flex min-h-0 flex-1 flex-col gap-3.5 overflow-y-auto p-3.5">
      <section className="fui-group">
        <div className="fui-group-head">CAPTURE</div>
        <div className="[&>*+*]:border-t [&>*+*]:border-hairline">
          <ToggleRow
            title="Capture clipboard changes"
            detail="Anything you copy on this device is encrypted locally, then relayed."
            onLabel="ON"
            offLabel="PAUSED"
            offClass="text-amber-400"
            checked={settings.capture_enabled}
            onChange={(capture_enabled) => update({ capture_enabled })}
            testId="capture-enabled"
          />
          <ToggleRow
            title="Launch at login"
            detail="Start the tray agent when this machine boots."
            onLabel="ON"
            offLabel="OFF"
            offClass="text-text-dim"
            checked={settings.autostart}
            onChange={(autostart) => update({ autostart })}
            testId="autostart"
          />
        </div>
      </section>

      <section className="fui-group">
        <div className="fui-group-head">GLOBAL HOTKEY</div>
        <div className="flex flex-col gap-2 p-3">
          <input
            data-testid="hotkey"
            className="fui-field"
            placeholder="unbound"
            value={hotkeyDraft}
            onChange={(e) => setHotkeyDraft(e.target.value)}
            onBlur={commitHotkey}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commitHotkey();
              }
            }}
          />
          <p className="m-0 text-data text-text-dim">{hotkeyHint}</p>
        </div>
      </section>

      {/* The only place an update is offered on this device besides the tray:
          ADR 0002 keeps it out of the popover, which is a picker. */}
      <section className="fui-group">
        <div className="fui-group-head">
          <span>UPDATES</span>
          <span className="font-mono text-chrome text-text-dim" data-testid="current-version">
            {version ? `V${version}` : "—"}
          </span>
        </div>
        <div className="[&>*+*]:border-t [&>*+*]:border-hairline">
          <ToggleRow
            title="Check at launch"
            detail="Asks github.com for the newest release when the app starts, revealing this machine's address, OS and version. Nothing about an entry, a key or a relay is sent."
            onLabel="ON"
            offLabel="OFF"
            offClass="text-text-dim"
            checked={settings.update_check_enabled}
            onChange={(update_check_enabled) => update({ update_check_enabled })}
            testId="update-check-enabled"
          />
          <div className="flex items-center justify-between gap-5 px-3 py-2.5">
            <div className="min-w-0">
              <div className="font-display text-sm font-medium tracking-phrase text-text-body">Check now</div>
              <div className="text-data text-text-muted">{updateHint}</div>
            </div>
            <button
              type="button"
              data-testid="check-for-update"
              disabled={checkState === "asking"}
              className="fui-action shrink-0 disabled:cursor-not-allowed disabled:text-text-dim"
              data-variant="outline"
              onClick={checkForUpdate}
            >
              Check
            </button>
          </div>
          {available && (
            <div className="flex flex-col gap-2 p-3">
              <div
                data-testid="update-offer"
                className="font-display text-sm font-medium tracking-phrase text-text-body"
              >
                Version {available.version} is ready to install
              </div>
              {/* Verbatim, wrapped: this is the changelog section the release
                  carries, written for a reader rather than lifted from commits. */}
              <pre
                data-testid="update-notes"
                className="fui-scroll m-0 max-h-40 overflow-y-auto whitespace-pre-wrap font-sans text-data text-text-muted"
              >
                {available.notes ?? "This release published no notes."}
              </pre>
              <button
                type="button"
                data-testid="install-update"
                className="fui-action self-start shrink-0"
                onClick={installUpdate}
              >
                Install and restart
              </button>
            </div>
          )}
        </div>
      </section>

      <section className="fui-group">
        <div className="fui-group-head">
          <span>DENY-LIST</span>
          <span className="font-mono text-chrome text-text-dim">
            {denyCount} {denyCount === 1 ? "APP" : "APPS"}
          </span>
        </div>
        <div className="flex flex-col gap-2 p-3">
          <textarea
            data-testid="deny-list"
            rows={4}
            className="fui-field"
            placeholder="com.1password.1password"
            value={settings.deny_list.join("\n")}
            onChange={(e) => update({ deny_list: e.target.value.split("\n").map((s) => s.trim()).filter(Boolean) })}
          />
          <p className="m-0 text-data text-text-dim">
            One bundle id per line. Copies made in these apps are never captured.
          </p>
        </div>
      </section>

      <section className="fui-group" data-tone="alert">
        <div className="fui-group-head" data-tone="alert">DESTRUCTIVE</div>
        <div className="flex items-center justify-between gap-5 px-3 py-2.5">
          <div className="min-w-0">
            <div className="font-display text-sm font-medium tracking-phrase text-text-body">Clear history</div>
            <div className="text-data text-text-muted">
              {activePairing
                ? `Deletes every stored entry for ${activePairing.username ?? activePairing.user_id} @ ${relayHost(activePairing.server_url)}, on the relay and on every paired device.`
                : "No active pairing."}
            </div>
          </div>
          <button
            type="button"
            data-testid="clear-history"
            disabled={!activeUserId}
            className="fui-action shrink-0 bg-alert-400 text-void-1000 focus-visible:outline-alert-400 enabled:hover:brightness-110 disabled:cursor-not-allowed disabled:bg-void-700 disabled:text-text-dim"
            onClick={() => setConfirmingClear(true)}
          >
            Clear…
          </button>
        </div>
        {/* Inline, never a dialog: the strip sits under the thing it is about, so the
            scope of the erase stays on screen while the choice is being made. */}
        {confirmingClear && (
          <Strip tone="alert" testId="confirm-strip-clear-history">
            <span className="shrink-0 border border-alert-400 px-1.5 py-0.5 font-mono text-chrome tracking-word">
              ALERT
            </span>
            <span className="flex-1 text-data normal-case tracking-phrase">
              Erase every stored entry for this pairing, on the relay and on all paired devices?
            </span>
            <button
              type="button"
              data-testid="cancel-clear-history"
              className="fui-action shrink-0"
              data-variant="outline"
              onClick={() => setConfirmingClear(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              data-testid="confirm-clear-history"
              className="fui-action shrink-0 bg-alert-400 text-void-1000 focus-visible:outline-alert-400 hover:brightness-110"
              onClick={clearHistory}
            >
              Clear history
            </button>
          </Strip>
        )}
      </section>

      {/* One error surface for the whole pane: load, patch and clear all land here. */}
      {error && (
        <Strip tone="alert" testId="settings-error">
          <span className="normal-case">{error}</span>
        </Strip>
      )}
    </div>
  );
}

/**
 * One switched setting: what it does, what it is doing now, and the control.
 *
 * The word readout is deliberately redundant with the switch. A 44px track only
 * reads as on or off once you know which end is which, and PAUSED is a state the
 * user has to recognise at a glance rather than decode from a knob position.
 */
function ToggleRow(props: {
  title: string;
  detail: string;
  onLabel: string;
  offLabel: string;
  /** Off is amber where it means capture is suppressed, dim where off is unremarkable. */
  offClass: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  testId: string;
}) {
  return (
    <div className="flex items-center justify-between gap-5 px-3 py-2.5">
      <div className="min-w-0">
        <div className="font-display text-sm font-medium tracking-phrase text-text-body">{props.title}</div>
        <div className="text-data text-text-muted">{props.detail}</div>
      </div>
      <div className="flex shrink-0 items-center gap-3">
        <span
          className={`w-14 text-right font-mono text-chrome tracking-word ${props.checked ? "text-cyan-300" : props.offClass}`}
        >
          {props.checked ? props.onLabel : props.offLabel}
        </span>
        {/* A real checkbox, visually hidden: the switch is painted by the sibling span,
            so the control keeps its role, its accessible name and keyboard behaviour. */}
        <label className="relative inline-flex cursor-pointer items-center">
          <input
            type="checkbox"
            data-testid={props.testId}
            aria-label={props.title}
            className="peer sr-only"
            checked={props.checked}
            onChange={(e) => props.onChange(e.target.checked)}
          />
          <span className="block h-5 w-11 border border-hairline bg-void-700 transition-colors duration-fast after:absolute after:left-[3px] after:top-[3px] after:h-[14px] after:w-[14px] after:bg-text-dim after:transition-transform after:duration-fast after:content-[''] peer-checked:border-emitter peer-checked:bg-cyan-800 peer-checked:after:translate-x-[24px] peer-checked:after:bg-cyan-300 peer-focus-visible:outline peer-focus-visible:outline-1 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-cyan-500" />
        </label>
      </div>
    </div>
  );
}
