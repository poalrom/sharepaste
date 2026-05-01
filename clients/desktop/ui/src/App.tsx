export default function App() {
  const route = document.body.dataset.route ?? "popover";
  const modal = document.body.dataset.modal ?? "";
  return (
    <div className="p-4 text-sm">
      <div>route: {route}</div>
      {route === "modal" ? <div>modal: {modal || "(unset)"}</div> : null}
    </div>
  );
}
