import Popover from "./views/Popover";
import PairingModal from "./modals/PairingModal";
import SettingsModal from "./modals/SettingsModal";
import AccountsModal from "./modals/AccountsModal";

export default function App() {
  const route = document.body.dataset.route ?? "popover";
  if (route === "modal") {
    const params = new URLSearchParams(window.location.search);
    const kind = params.get("kind") ?? "";
    if (kind === "pairing")  return <PairingModal />;
    if (kind === "settings") return <SettingsModal />;
    if (kind === "accounts") return <AccountsModal />;
    return <div>Unknown modal: {kind}</div>;
  }
  return <Popover />;
}
