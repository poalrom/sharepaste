import type { ReactNode } from "react";
import type { Tone } from "./tone";

type Props = {
  tone: Tone;
  children: ReactNode;
  testId?: string;
};

/**
 * A chrome band across the panel.
 *
 * Strips are the popover's only telemetry surface and they are conditional by
 * design (ADR 0002): a healthy window shows none of them, so every strip on
 * screen is carrying news.
 */
export default function Strip({ tone, children, testId }: Props) {
  return (
    <div className="fui-band shrink-0">
      <div className="fui-strip" data-tone={tone} data-testid={testId} role="status">
        {children}
      </div>
    </div>
  );
}
