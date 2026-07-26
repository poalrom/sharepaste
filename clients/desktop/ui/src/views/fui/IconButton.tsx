import type { MouseEvent, ReactNode } from "react";

type Props = {
  /** Required: every control in the shell is a bare glyph, so nothing else names it. */
  label: string;
  onClick: (e: MouseEvent<HTMLButtonElement>) => void;
  children: ReactNode;
  /** Hover text, when the button's effect differs from its `label`. */
  title?: string;
  tone?: "default" | "alert";
  testId?: string;
  className?: string;
};

export default function IconButton({
  label,
  onClick,
  children,
  title,
  tone = "default",
  testId,
  className = "",
}: Props) {
  return (
    <button
      type="button"
      aria-label={label}
      title={title}
      data-tone={tone}
      data-testid={testId}
      onClick={onClick}
      className={`fui-icon-button h-5 w-5 shrink-0 ${className}`}
    >
      {children}
    </button>
  );
}
