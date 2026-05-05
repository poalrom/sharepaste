import { useEffect } from "react";
import { useUiStore, type MainSection } from "../store/ui";
import { events } from "../ipc/events";
import AccountsSection from "./sections/AccountsSection";
import SettingsSection from "./sections/SettingsSection";
import PairingSection from "./sections/PairingSection";

const SECTIONS: MainSection[] = ["accounts", "settings", "pairing"];
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
    if (fromUrl && (SECTIONS as string[]).includes(fromUrl)) {
      setActive(fromUrl as MainSection);
    }
  }, [setActive]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      const off = await events.onMainNavigate((section) => {
        if ((SECTIONS as string[]).includes(section)) setActive(section as MainSection);
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
        {SECTIONS.map((s) => (
          <button
            key={s}
            data-testid={`tab-${s}`}
            role="tab"
            aria-selected={active === s}
            className={
              "px-4 py-2 text-sm " +
              (active === s
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
        {active === "accounts" && <AccountsSection />}
        {active === "settings" && <SettingsSection />}
        {active === "pairing" && <PairingSection />}
      </div>
    </div>
  );
}
