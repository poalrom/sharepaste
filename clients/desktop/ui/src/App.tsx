import Popover from "./views/Popover";
import PairingSection from "./views/sections/PairingSection";
import SettingsSection from "./views/sections/SettingsSection";
import AccountsSection from "./views/sections/AccountsSection";

export default function App() {
  const route = document.body.dataset.route ?? "popover";
  if (route === "modal") {
    const params = new URLSearchParams(window.location.search);
    const kind = params.get("kind") ?? "";
    if (kind === "pairing")  return <PairingSection />;
    if (kind === "settings") return <SettingsSection />;
    if (kind === "accounts") return <AccountsSection />;
    return <div>Unknown modal: {kind}</div>;
  }
  return <Popover />;
}
