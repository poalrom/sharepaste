import { useEffect } from "react";
import { useUiStore, type MainSection } from "../store/ui";
import { events } from "../ipc/events";
import AccountsSection from "./sections/AccountsSection";
import SettingsSection from "./sections/SettingsSection";

const ROUTABLE_SECTIONS: MainSection[] = ["accounts", "settings", "pairing"];
const TABS: Array<Exclude<MainSection, "pairing">> = ["accounts", "settings"];
const LABELS: Record<MainSection, string> = {
  accounts: "Accounts",
  settings: "Settings",
  pairing: "Pairing",
};

export default function Main() {
  const active = useUiStore((s) => s.mainSection);
  const setActive = useUiStore((s) => s.setMainSection);

  useEffect(() => {
    const fromUrl = new URLSearchParams(window.location.search).get("section");
    if (fromUrl && (ROUTABLE_SECTIONS as string[]).includes(fromUrl)) {
      setActive(fromUrl as MainSection);
    }
  }, [setActive]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const off = await events.onMainNavigate((section) => {
        if ((ROUTABLE_SECTIONS as string[]).includes(section)) setActive(section as MainSection);
      });
      if (cancelled) off();
      else unsub = off;
    })();
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [setActive]);

  return (
    <div className="flex h-full flex-col">
      <nav role="tablist" className="flex border-b border-zinc-700">
        {TABS.map((s) => (
          <button
            key={s}
            data-testid={`tab-${s}`}
            role="tab"
            aria-selected={active === s || (s === "accounts" && active === "pairing")}
            className={
              "px-4 py-2 text-sm " +
              (active === s || (s === "accounts" && active === "pairing")
                ? "border-b-2 border-blue-500 text-blue-300"
                : "text-zinc-300 hover:text-zinc-100")
            }
            onClick={() => setActive(s)}
          >
            {LABELS[s]}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-auto">
        {(active === "accounts" || active === "pairing") && <AccountsSection />}
        {active === "settings" && <SettingsSection />}
      </div>
    </div>
  );
}
