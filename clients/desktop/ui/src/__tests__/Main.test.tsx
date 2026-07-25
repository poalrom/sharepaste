import { describe, expect, it, beforeEach, afterEach } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { mockIpc, type MockIpc } from "./helpers";
import { useUiStore, type MainSection } from "../store/ui";
import Main from "../views/Main";

let ipc: MockIpc;

beforeEach(() => {
  ipc = mockIpc({
    invoke: (command) => {
      if (command === "get_settings") {
        return { capture_enabled: true, deny_list: [], autostart: false, hotkey: null };
      }
      if (command === "list_accounts") return [];
      return undefined;
    },
  });
  useUiStore.setState({ search: "", selectedIndex: 0, mainSection: "accounts" });
});

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

/**
 * Drains the sections' async mount effects inside act(). Without this React
 * warns about the state updates that land after the test body returns.
 */
const settle = () => act(async () => {});

async function renderMain() {
  const view = render(<Main />);
  await settle();
  return view;
}

describe("Main shell", () => {
  it("routes the initial section from ?section=, falling back to accounts", async () => {
    const cases: Array<{ url: string; section: MainSection; selectedTab: string }> = [
      { url: "/main.html?section=settings", section: "settings", selectedTab: "tab-settings" },
      { url: "/main.html?section=pairing", section: "pairing", selectedTab: "tab-accounts" },
      { url: "/main.html", section: "accounts", selectedTab: "tab-accounts" },
      { url: "/main.html?section=nonsense", section: "accounts", selectedTab: "tab-accounts" },
    ];

    for (const { url, section, selectedTab } of cases) {
      useUiStore.setState({ mainSection: "accounts" });
      window.history.replaceState({}, "", url);

      const view = await renderMain();
      expect(useUiStore.getState().mainSection).toBe(section);
      expect(view.getByTestId(selectedTab)).toHaveAttribute("aria-selected", "true");
      // Pairing has no tab of its own: it routes under the accounts tab.
      if (section === "pairing") expect(view.getByText("How are you pairing?")).toBeInTheDocument();

      view.unmount();
    }
  });

  it("clicking a tab updates the active section", async () => {
    await renderMain();
    fireEvent.click(screen.getByTestId("tab-settings"));
    expect(useUiStore.getState().mainSection).toBe("settings");
    await settle();
  });

  it("does not render a separate pairing tab", async () => {
    await renderMain();
    expect(screen.queryByTestId("tab-pairing")).toBeNull();
  });

  it("main://navigate event flips the active section", async () => {
    await renderMain();
    expect(ipc.handlers.get("main://navigate")).toHaveLength(1);

    await act(async () => ipc.emit("main://navigate", "settings"));
    expect(useUiStore.getState().mainSection).toBe("settings");
    await settle();
  });
});
