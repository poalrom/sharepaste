import { useState, useEffect } from "react";
import { cmd } from "../../ipc/commands";
import { events } from "../../ipc/events";
import { useUiStore } from "../../store";
import type { AppErrorPayload } from "../../types";
import { IconButton, Strip } from "../fui";

type Step = "chooser" | "invite" | "code" | "show-code" | "paired";

/** What the card's header band calls the step it is showing. */
const STEP_LABEL: Record<Step, string> = {
  chooser: "CHOOSE",
  invite: "INVITE",
  code: "PAIR CODE",
  "show-code": "SHOW CODE",
  paired: "PAIRED",
};

/**
 * A shortcode and the window it is claimable in.
 *
 * The payload carries only the deadline and the two-minute TTL is decided in
 * `commands.rs`, so without the moment the code was issued the drain bar has no
 * denominator that is not a second copy of that constant.
 */
type CodeWindow = { code: string; issued: number; expires: number };

export default function PairingFlow({
  forUserId,
  onClose,
}: {
  /** Pair another device onto *this* pairing: no chooser, straight to the code. */
  forUserId?: string;
  /** Passed by the card that owns the panel; the standalone flow closes the flag. */
  onClose?: () => void;
}) {
  const setPairingFlowOpen = useUiStore((s) => s.setPairingFlowOpen);
  const close = onClose ?? (() => setPairingFlowOpen(false));
  // `+ DEVICE` opens onto the code, and there is no chooser behind it to go back to.
  const firstStep: Step = forUserId === undefined ? "chooser" : "show-code";

  const [step, setStep] = useState<Step>(firstStep);
  const [serverUrl, setServerUrl] = useState("https://");
  const [token, setToken] = useState("");
  const [code, setCode] = useState("");
  // Keep it simple — engineer can swap to navigator.platform + a random suffix later.
  const [label, setLabel] = useState("macbook");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [codeWindow, setCodeWindow] = useState<CodeWindow>();
  const [pairedDeviceLabel, setPairedDeviceLabel] = useState<string>();

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    (async () => {
      unsubs.push(await events.onPairShortcode((issued) => {
        setCodeWindow({ code: issued.code, issued: Date.now(), expires: issued.expires_at });
        setStep("show-code");
      }));
      unsubs.push(await events.onPairClaimed(({ device_label }) => {
        setError(undefined);
        setPairedDeviceLabel(device_label ?? undefined);
        setStep("paired");
      }));
      unsubs.push(await events.onPairExpired(() => setError("Pair code expired or already used. Generate a new one.")));
    })();
    return () => unsubs.forEach((u) => u());
  }, []);

  /*
   * `pair_start` takes any user id; the flow used to restrict show-code to the
   * Active Pairing for no reason the command shares (plan §7). The card names
   * the pairing, so the request is the mount rather than a click.
   */
  useEffect(() => {
    if (forUserId === undefined) return;
    let cancelled = false;
    cmd.pairStart({ user_id: forUserId })
      .then((started) => {
        if (!cancelled) {
          setCodeWindow({ code: started.code, issued: Date.now(), expires: started.expires_at });
        }
      })
      .catch((e: unknown) => { if (!cancelled) setError(messageOf(e)); });
    return () => { cancelled = true; };
  }, [forUserId]);

  const handle = async (fn: () => Promise<unknown>) => {
    setBusy(true); setError(undefined);
    try { await fn(); }
    catch (e) { setError(messageOf(e)); }
    finally { setBusy(false); }
  };

  const insecure = /^http:\/\/(?!localhost|127\.0\.0\.1)/i.test(serverUrl);
  const codeIsValid = /^[A-Z2-7\s\-]+$/i.test(code.trim()) && code.replace(/\s|-/g, "").length >= 80;

  return (
    <div className="flex min-w-0 flex-col">
      {/* The band exists only inside a card, where the flow is one panel among
          several and has to name the step it is on and the way out of it. */}
      {forUserId !== undefined && (
        <div className="fui-group-head">
          <span>PAIR A DEVICE</span>
          <span className="flex items-center gap-2.5">
            <span className="font-mono text-chrome tracking-phrase text-text-dim">{STEP_LABEL[step]}</span>
            <IconButton
              label="Close the pairing panel"
              testId={`pair-panel-close-${forUserId}`}
              onClick={close}
            >
              <span aria-hidden="true" className="text-chrome leading-none">✕</span>
            </IconButton>
          </span>
        </div>
      )}

      <div className="flex flex-col gap-3 p-3.5">
        {step === "chooser" && (
          <>
            <p className="m-0 text-label uppercase tracking-word text-text-muted">How are you pairing?</p>
            <button
              type="button"
              data-testid="choose-invite"
              className="fui-group flex flex-col gap-1 px-3 py-2.5 text-left text-text-body transition-colors duration-fast hover:border-emitter hover:text-cyan-300"
              onClick={() => setStep("invite")}
            >
              <span className="font-display text-sm font-medium tracking-phrase">I have an invite token</span>
              <span className="text-data text-text-dim">An operator issued a token for a new pairing.</span>
            </button>
            <button
              type="button"
              data-testid="choose-code"
              className="fui-group flex flex-col gap-1 px-3 py-2.5 text-left text-text-body transition-colors duration-fast hover:border-emitter hover:text-cyan-300"
              onClick={() => setStep("code")}
            >
              <span className="font-display text-sm font-medium tracking-phrase">I have a pair code</span>
              <span className="text-data text-text-dim">Another of my devices is showing a short code.</span>
            </button>
          </>
        )}

        {step === "invite" && (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              handle(async () => {
                await cmd.pairWithInvite({ server_url: serverUrl, token, device_label: label });
                close();
              });
            }}
          >
            <p className="m-0 text-label uppercase tracking-word text-text-muted">Claim invite</p>
            {insecure && (
              <Strip tone="caution" testId="insecure-warning">
                CAUTION · Unencrypted transport — trusted networks only.
              </Strip>
            )}
            <label className="flex flex-col gap-1.5">
              <span className="text-label uppercase tracking-word text-text-muted">Relay URL</span>
              <input className="fui-field" value={serverUrl} onChange={(e) => setServerUrl(e.target.value)} />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-label uppercase tracking-word text-text-muted">Invite token</span>
              <input className="fui-field" value={token} onChange={(e) => setToken(e.target.value)} />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-label uppercase tracking-word text-text-muted">Device Label</span>
              <input className="fui-field" value={label} onChange={(e) => setLabel(e.target.value)} />
            </label>
            <div className="flex items-center gap-2">
              <button type="button" className="fui-action" data-variant="outline" onClick={() => setStep("chooser")}>
                ‹ BACK
              </button>
              <button
                type="submit"
                disabled={busy}
                className="fui-action disabled:cursor-not-allowed disabled:opacity-50"
                data-variant="solid"
              >
                CLAIM
              </button>
            </div>
          </form>
        )}

        {step === "code" && (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              handle(async () => {
                await cmd.pairWithCode({ code, device_label: label });
                close();
              });
            }}
          >
            <p className="m-0 text-label uppercase tracking-word text-text-muted">Add this device</p>
            <label className="flex flex-col gap-1.5">
              <span className="text-label uppercase tracking-word text-text-muted">Pair code</span>
              <textarea
                rows={4}
                data-testid="pair-code"
                className="fui-field"
                data-invalid={code !== "" && !codeIsValid}
                value={code}
                onChange={(e) => setCode(e.target.value)}
              />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="text-label uppercase tracking-word text-text-muted">Device Label</span>
              <input className="fui-field" value={label} onChange={(e) => setLabel(e.target.value)} />
            </label>
            {code !== "" && !codeIsValid && (
              <p className="m-0 text-data tracking-phrase text-alert-400">
                That doesn't look like a valid pair code.
              </p>
            )}
            <div className="flex items-center gap-2">
              <button type="button" className="fui-action" data-variant="outline" onClick={() => setStep("chooser")}>
                ‹ BACK
              </button>
              <button
                type="submit"
                disabled={busy || !codeIsValid}
                className="fui-action disabled:cursor-not-allowed disabled:opacity-50"
                data-variant="solid"
              >
                PAIR
              </button>
            </div>
          </form>
        )}

        {step === "show-code" && (
          <>
            <p className="m-0 text-label uppercase tracking-word text-text-muted">
              Show this code on the new device
            </p>
            <pre
              data-testid="shortcode"
              className={`m-0 whitespace-pre-wrap break-words border border-emitter bg-void-1000 px-3 py-4 text-center font-mono text-2xl tracking-[0.18em] ${
                codeWindow ? "text-cyan-300" : "text-text-dim"
              }`}
            >
              {codeWindow?.code ?? "REQUESTING…"}
            </pre>
            <Countdown codeWindow={codeWindow} />
            {/* No chooser behind a card-owned panel: its header band holds the way out. */}
            {forUserId === undefined && (
              <div className="flex items-center gap-2">
                <button type="button" className="fui-action" data-variant="outline" onClick={() => setStep("chooser")}>
                  ‹ BACK
                </button>
              </div>
            )}
          </>
        )}

        {step === "paired" && (
          <>
            <p className="m-0 text-label uppercase tracking-word text-nominal-400">
              {pairedDeviceLabel ? `Paired a new device "${pairedDeviceLabel}"` : "Paired a new device"}
            </p>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="fui-action"
                data-variant="solid"
                onClick={() => {
                  setStep(firstStep);
                  close();
                }}
              >
                DONE
              </button>
            </div>
          </>
        )}
      </div>

      {error !== undefined && (
        <Strip tone="alert" testId="pair-error">
          <span className="shrink-0">ALERT</span>
          <span className="min-w-0 normal-case">{error}</span>
        </Strip>
      )}
    </div>
  );
}

/**
 * How long the shown code stays claimable.
 *
 * A bar as well as a clock, because the person reading this is looking at the
 * code to retype it, not at the digits beside it; the drain from cyan through
 * amber to alert is legible without being read.
 */
function Countdown({ codeWindow }: { codeWindow: CodeWindow | undefined }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const i = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(i);
  }, []);
  if (!codeWindow) return null;
  const remaining = Math.max(0, codeWindow.expires - now);
  const fraction = Math.min(1, remaining / Math.max(1, codeWindow.expires - codeWindow.issued));
  const seconds = Math.ceil(remaining / 1000);
  return (
    <div className="flex flex-col gap-1.5" data-testid="countdown">
      <span className="text-chrome uppercase tracking-word text-amber-400">
        {`EXPIRES IN ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`}
      </span>
      <div className="h-0.5 w-full bg-void-700">
        <div
          className={`h-0.5 transition-[width] duration-mid ${
            fraction > 0.5 ? "bg-cyan-500" : fraction > 0.2 ? "bg-amber-400" : "bg-alert-400"
          }`}
          style={{ width: `${fraction * 100}%` }}
        />
      </div>
    </div>
  );
}

function messageOf(e: unknown): string {
  if (typeof e === "object" && e && "message" in e && typeof (e as AppErrorPayload).message === "string") {
    return (e as AppErrorPayload).message;
  }
  return String(e);
}
