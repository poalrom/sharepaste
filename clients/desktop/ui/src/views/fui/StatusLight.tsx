import type { Tone } from "./tone";

type Props = {
  tone: Tone;
  /** Reads the state aloud; the dot itself is decorative. */
  label: string;
  /**
   * Marks a state that is still resolving. The animation is suppressed under
   * `prefers-reduced-motion: reduce` by `styles.css`, not by a media query
   * duplicated here.
   */
  pulse?: boolean;
  testId?: string;
};

export default function StatusLight({ tone, label, pulse = false, testId }: Props) {
  return (
    <span className="inline-flex items-center gap-1.5" data-testid={testId}>
      <span className="fui-light" data-tone={tone} data-pulse={pulse} aria-hidden="true" />
      <span className="tracking-phrase">{label}</span>
    </span>
  );
}
