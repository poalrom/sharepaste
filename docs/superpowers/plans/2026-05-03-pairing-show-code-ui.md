# Pairing Show-Code UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the missing desktop UI path that lets an already-paired Sharepaste device generate and display a short pair code for another device.

**Architecture:** Keep pairing in the existing `PairingModal`. The modal reads the active account from the existing Zustand accounts store, always renders the show-code option, disables it when no active account exists, and calls the existing `cmd.pairStart({ user_id })` command when available. The existing `show-code` view and pairing events remain the display and lifecycle mechanism.

**Tech Stack:** React 18, TypeScript, Zustand, Tauri IPC wrapper, Vitest, Testing Library.

---

## File Structure

- Modify: `clients/desktop/ui/src/modals/PairingModal.tsx`
  - Owns the pairing chooser and flow-specific modal screens.
  - Will import `useAccountsStore`, read `active`, render the third chooser option, and call `cmd.pairStart`.

- Modify: `clients/desktop/ui/src/__tests__/PairingModal.test.tsx`
  - Owns focused component coverage for the modal.
  - Will seed `useAccountsStore` for active-account tests and assert `pair_start` IPC behavior.

No server, Rust, storage, or wire-protocol files are part of this plan.

---

### Task 1: Add Failing Tests For Show-Code Chooser Behavior

**Files:**
- Modify: `clients/desktop/ui/src/__tests__/PairingModal.test.tsx`

- [ ] **Step 1: Replace the current test file with coverage for the new option**

Replace the full contents of `clients/desktop/ui/src/__tests__/PairingModal.test.tsx` with:

```tsx
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useAccountsStore } from "../store/accounts";
import PairingModal from "../modals/PairingModal";

let invoke: ReturnType<typeof vi.fn<Invoker>>;

beforeEach(() => {
  invoke = vi.fn(async (cmd) => {
    if (cmd === "pair_start") return { code: "ABCDE FGHIJ", expires_at: Date.now() + 120_000 };
    return { user_id: "u", device_id: "d" };
  }) as ReturnType<typeof vi.fn<Invoker>>;
  const listen = vi.fn(async () => () => {}) as ReturnType<typeof vi.fn<Listener>>;
  injectForTests(invoke as never, listen as never);
  useAccountsStore.setState({ accounts: [], active: undefined });
});

describe("PairingModal", () => {
  it("starts on the chooser screen", () => {
    render(<PairingModal />);
    expect(screen.getByTestId("choose-invite")).toBeInTheDocument();
    expect(screen.getByTestId("choose-code")).toBeInTheDocument();
    expect(screen.getByTestId("choose-show-code")).toBeInTheDocument();
  });

  it("keeps the show-code option disabled without an active account", () => {
    render(<PairingModal />);
    const showCode = screen.getByTestId("choose-show-code");
    expect(showCode).toBeDisabled();
    expect(screen.getByText(/Pair this device first/i)).toBeInTheDocument();
  });

  it("navigates to the invite step", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    expect(screen.getByText(/Claim invite/i)).toBeInTheDocument();
  });

  it("warns on plain http to non-localhost", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    const url = screen.getByLabelText(/Server URL/i, { selector: "input" }) as HTMLInputElement;
    fireEvent.change(url, { target: { value: "http://example.com" } });
    expect(screen.getByTestId("insecure-warning")).toBeInTheDocument();
  });

  it("shows red border on invalid pair code", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-code"));
    const ta = screen.getByTestId("pair-code") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "tiny" } });
    expect(ta.className).toContain("ring-red-500");
  });

  it("starts pairing for the active account and displays the returned code", async () => {
    useAccountsStore.setState({
      accounts: [
        { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Online", pending: 0 },
      ],
      active: "u-active",
    });

    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-show-code"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("pair_start", { args: { user_id: "u-active" } });
    });
    expect(await screen.findByTestId("shortcode")).toHaveTextContent("ABCDE FGHIJ");
    expect(screen.getByTestId("countdown")).toBeInTheDocument();
  });

  it("shows a chooser error when starting pairing fails", async () => {
    invoke.mockImplementationOnce(async () => {
      throw { kind: "Network", message: "server unavailable" };
    });
    useAccountsStore.setState({
      accounts: [
        { user_id: "u-active", device_id: "d1", label: "Laptop", server_url: "https://srv", status: "Online", pending: 0 },
      ],
      active: "u-active",
    });

    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-show-code"));

    expect(await screen.findByText("server unavailable")).toBeInTheDocument();
    expect(screen.getByTestId("choose-show-code")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the focused test and verify it fails for the missing UI**

Run:

```powershell
npm.cmd --prefix clients/desktop/ui test -- PairingModal.test.tsx
```

Expected: FAIL. At least one failure should mention that `choose-show-code` cannot be found.

- [ ] **Step 3: Commit the failing tests**

Run:

```bash
git add clients/desktop/ui/src/__tests__/PairingModal.test.tsx
git commit -m "test(desktop): cover show-code pairing chooser"
```

---

### Task 2: Wire PairingModal To Active Account And pair_start

**Files:**
- Modify: `clients/desktop/ui/src/modals/PairingModal.tsx`

- [ ] **Step 1: Replace `PairingModal.tsx` with the wired chooser implementation**

Replace the full contents of `clients/desktop/ui/src/modals/PairingModal.tsx` with:

```tsx
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
        {insecure && <div data-testid="insecure-warning" className="rounded border border-amber-600 bg-amber-900/30 px-2 py-1 text-xs text-amber-300">Unencrypted - only use on trusted networks.</div>}
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
  return "macbook";
}

function messageOf(e: unknown): string {
  if (typeof e === "object" && e && "message" in e && typeof (e as AppErrorPayload).message === "string") {
    return (e as AppErrorPayload).message;
  }
  return String(e);
}
```

- [ ] **Step 2: Run the focused test and verify it passes**

Run:

```powershell
npm.cmd --prefix clients/desktop/ui test -- PairingModal.test.tsx
```

Expected: PASS. Output should show `PairingModal.test.tsx` passing all tests.

- [ ] **Step 3: Commit the UI implementation**

Run:

```bash
git add clients/desktop/ui/src/modals/PairingModal.tsx
git commit -m "feat(desktop): show pair code from pairing modal"
```

---

### Task 3: Run Full UI Verification

**Files:**
- Verify: `clients/desktop/ui/src/modals/PairingModal.tsx`
- Verify: `clients/desktop/ui/src/__tests__/PairingModal.test.tsx`

- [ ] **Step 1: Run the full UI test suite**

Run:

```powershell
npm.cmd --prefix clients/desktop/ui test
```

Expected: PASS. All Vitest suites complete successfully.

- [ ] **Step 2: Run the production UI build**

Run:

```powershell
npm.cmd --prefix clients/desktop/ui run build
```

Expected: PASS. TypeScript and Vite complete successfully, and the output includes `built in`.

- [ ] **Step 3: Check the worktree**

Run:

```powershell
git status --short
```

Expected: clean, except for unrelated files that existed before this plan.

- [ ] **Step 4: Confirm no verification commit is needed**

The UI test and build commands should not modify tracked files. If `git status --short` shows no changes from verification, make no commit.

Expected: no commit is needed for this task.
