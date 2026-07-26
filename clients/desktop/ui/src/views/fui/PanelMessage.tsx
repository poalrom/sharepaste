type Action = {
  label: string;
  onClick: () => void;
  /** Solid marks the one thing to do here; outline marks a choice among several. */
  variant?: "solid" | "outline";
  testId?: string;
};

type Props = {
  /** Single-word-ish headline; rendered in wide tracking. */
  title: string;
  /** Optional sentence explaining the state in plain language. */
  detail?: string;
  action?: Action;
  testId?: string;
};

/** Fills the list area when there are no rows to show. */
export default function PanelMessage({ title, detail, action, testId }: Props) {
  return (
    <div
      data-testid={testId}
      className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center"
    >
      <p className="m-0 text-label uppercase tracking-word text-text-muted">{title}</p>
      {detail && <p className="m-0 text-chrome tracking-phrase text-text-dim">{detail}</p>}
      {action && (
        <button
          type="button"
          data-testid={action.testId}
          onClick={action.onClick}
          className="fui-action"
          data-variant={action.variant ?? "solid"}
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
