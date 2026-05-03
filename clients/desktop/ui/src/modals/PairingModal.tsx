import { useState, useEffect } from "react";
import { cmd } from "../ipc/commands";
import { events } from "../ipc/events";
import { useAccountsStore } from "../store/accounts";
import type { AppErrorPayload } from "../types";

type Step = "chooser" | "invite" | "code" | "show-code";

export default function PairingModal({ onClose }: { onClose?: () => void } = {}) {
  const close = onClose ?? (() => window.close());
  const activeUserId = useAccountsStore((s) => s.active);
  const [step, setStep] = useState<Step>("chooser");
  const [serverUrl, setServerUrl] = useState("https://");
  const [token, setToken] = useState("");
  const [code, setCode] = useState("");
  const [label, setLabel] = useState(defaultLabel());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [shortcode, setShortcode] = useState<string>();
  const [expiresAt, setExpiresAt] = useState<number>();

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    (async () => {
      unsubs.push(await events.onPairShortcode(({ code, expires_at }) => {
        setShortcode(code);
        setExpiresAt(expires_at);
        setStep("show-code");
      }));
      unsubs.push(await events.onPairClaimed(() => { setError(undefined); close(); }));
      unsubs.push(await events.onPairExpired(() => setError("Pair code expired or already used. Generate a new one.")));
    })();
    return () => unsubs.forEach((u) => u());
  }, [close]);

  const handle = async (fn: () => Promise<unknown>) => {
    setBusy(true); setError(undefined);
    try { await fn(); }
    catch (e) { setError(messageOf(e)); }
    finally { setBusy(false); }
  };

  const startShowCode = () => {
    if (!activeUserId) return;
    handle(async () => {
      const started = await cmd.pairStart({ user_id: activeUserId });
      setShortcode(started.code);
      setExpiresAt(started.expires_at);
      setStep("show-code");
    });
  };

  if (step === "chooser") {
    const showCodeDisabled = busy || !activeUserId;
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-base font-semibold">How are you pairing?</h1>
        <button data-testid="choose-invite" className="rounded border border-zinc-700 p-3 text-left hover:bg-zinc-800" onClick={() => setStep("invite")}>
          <div className="font-semibold">I have an invite token</div>
          <div className="text-xs text-zinc-400">Operator gave me a token for a new account.</div>
        </button>
        <button data-testid="choose-code" className="rounded border border-zinc-700 p-3 text-left hover:bg-zinc-800" onClick={() => setStep("code")}>
          <div className="font-semibold">I have a pair code</div>
          <div className="text-xs text-zinc-400">Another of my devices is showing a short code.</div>
        </button>
        <button
          data-testid="choose-show-code"
          disabled={showCodeDisabled}
          className="rounded border border-zinc-700 p-3 text-left hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
          onClick={startShowCode}
        >
          <div className="font-semibold">I want to pair another device</div>
          <div className="text-xs text-zinc-400">
            {activeUserId ? "Show a short code from this account." : "Pair this device first before showing a code."}
          </div>
        </button>
        {error && <div className="text-xs text-red-400">{error}</div>}
      </div>
    );
  }

  if (step === "invite") {
    const insecure = /^http:\/\/(?!localhost|127\.0\.0\.1)/i.test(serverUrl);
    return (
      <form
        className="flex flex-col gap-3 p-6"
        onSubmit={(e) => {
          e.preventDefault();
          handle(async () => {
            await cmd.pairWithInvite({ server_url: serverUrl, token, device_label: label });
            close();
          });
        }}
      >
        <h1 className="text-base font-semibold">Claim invite</h1>
        {insecure && <div data-testid="insecure-warning" className="rounded border border-amber-600 bg-amber-900/30 px-2 py-1 text-xs text-amber-300">Unencrypted — only use on trusted networks.</div>}
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Server URL
          <input className="rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100" value={serverUrl} onChange={(e) => setServerUrl(e.target.value)} />
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Invite token
          <input className="rounded bg-zinc-800 px-2 py-1 font-mono text-sm text-zinc-100" value={token} onChange={(e) => setToken(e.target.value)} />
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Device label
          <input className="rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100" value={label} onChange={(e) => setLabel(e.target.value)} />
        </label>
        {error && <div className="text-xs text-red-400">{error}</div>}
        <div className="flex gap-2">
          <button type="button" className="rounded px-3 py-1 hover:underline" onClick={() => setStep("chooser")}>Back</button>
          <button type="submit" disabled={busy} className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50">Claim</button>
        </div>
      </form>
    );
  }

  if (step === "code") {
    const codeIsValid = /^[A-Z2-7\s\-]+$/i.test(code.trim()) && code.replace(/\s|-/g, "").length >= 80;
    return (
      <form
        className="flex flex-col gap-3 p-6"
        onSubmit={(e) => {
          e.preventDefault();
          handle(async () => {
            await cmd.pairWithCode({ code, device_label: label });
            close();
          });
        }}
      >
        <h1 className="text-base font-semibold">Add this device</h1>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Pair code
          <textarea
            rows={4}
            data-testid="pair-code"
            className={`rounded bg-zinc-800 px-2 py-1 font-mono text-sm text-zinc-100 ${code && !codeIsValid ? "ring-1 ring-red-500" : ""}`}
            value={code}
            onChange={(e) => setCode(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs text-zinc-400">
          Device label
          <input className="rounded bg-zinc-800 px-2 py-1 text-sm text-zinc-100" value={label} onChange={(e) => setLabel(e.target.value)} />
        </label>
        {code && !codeIsValid && <div className="text-xs text-red-400">That doesn't look like a valid pair code.</div>}
        {error && <div className="text-xs text-red-400">{error}</div>}
        <div className="flex gap-2">
          <button type="button" className="rounded px-3 py-1 hover:underline" onClick={() => setStep("chooser")}>Back</button>
          <button type="submit" disabled={busy || !codeIsValid} className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50">Pair</button>
        </div>
      </form>
    );
  }

  return (
    <div className="flex flex-col gap-3 p-6">
      <h1 className="text-base font-semibold">Show this code on the new device</h1>
      <pre data-testid="shortcode" className="whitespace-pre-wrap rounded bg-zinc-800 p-3 font-mono text-xs">{shortcode}</pre>
      <Countdown expiresAt={expiresAt} />
      {error && <div className="text-xs text-red-400">{error}</div>}
    </div>
  );
}

function Countdown({ expiresAt }: { expiresAt: number | undefined }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const i = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(i);
  }, []);
  if (!expiresAt) return null;
  const remaining = Math.max(0, Math.ceil((expiresAt - now) / 1000));
  return <div data-testid="countdown" className="text-xs text-zinc-400">Expires in {remaining}s</div>;
}

function defaultLabel(): string {
  // Keep it simple — engineer can swap to navigator.platform + a random suffix later.
  return "macbook";
}

function messageOf(e: unknown): string {
  if (typeof e === "object" && e && "message" in e && typeof (e as AppErrorPayload).message === "string") {
    return (e as AppErrorPayload).message;
  }
  return String(e);
}
