import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { injectForTests, type Invoker, type Listener } from "../ipc/tauri";
import { useUiStore } from "../store/ui";
import Main from "../views/Main";

let invoke: ReturnType<typeof vi.fn<Invoker>>;
let navigateCb: ((section: string) => void) | undefined;

beforeEach(() => {
  invoke = vi.fn(async () => undefined as never) as ReturnType<typeof vi.fn<Invoker>>;
  const listen: Listener = async <P,>(event: string, cb: (payload: P) => void) => {
    if (event === "main://navigate") navigateCb = cb as (s: string) => void;
    return () => {};
  };
  injectForTests(invoke as never, listen);
  useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
  navigateCb = undefined;
});

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

describe("Main shell", () => {
  it("uses ?section= from URL on mount", () => {
    window.history.replaceState({}, "", "/main.html?section=settings");
    render(<Main />);
    expect(useUiStore.getState().mainSection).toBe("settings");
    expect(screen.getByTestId("tab-settings")).toHaveAttribute("aria-selected", "true");
  });

  it("falls back to 'accounts' when ?section= is missing or unknown", () => {
    window.history.replaceState({}, "", "/main.html");
    render(<Main />);
    expect(useUiStore.getState().mainSection).toBe("accounts");
  });

  it("clicking a tab updates the active section", () => {
    render(<Main />);
    fireEvent.click(screen.getByTestId("tab-settings"));
    expect(useUiStore.getState().mainSection).toBe("settings");
  });

  it("does not render a separate pairing tab", () => {
    render(<Main />);
    expect(screen.queryByTestId("tab-pairing")).toBeNull();
  });

  it("shows pairing routes under the accounts tab", () => {
    window.history.replaceState({}, "", "/main.html?section=pairing");
    render(<Main />);
    expect(screen.getByTestId("tab-accounts")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("How are you pairing?")).toBeInTheDocument();
  });

  it("main://navigate event flips the active section", async () => {
    render(<Main />);
    // wait one microtask for the listener registration
    await Promise.resolve();
    expect(navigateCb).toBeDefined();
    navigateCb!("settings");
    expect(useUiStore.getState().mainSection).toBe("settings");
  });
});
