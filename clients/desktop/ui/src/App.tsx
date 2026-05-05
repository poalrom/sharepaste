import Popover from "./views/Popover";
import Main from "./views/Main";

export default function App() {
  const route = document.body.dataset.route ?? "popover";
  if (route === "main") return <Main />;
  return <Popover />;
}
