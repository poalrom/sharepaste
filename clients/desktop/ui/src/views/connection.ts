import type { ConnectionState } from "../types";
import type { Tone } from "./fui/tone";

export type ConnectionReadout = {
  tone: Tone;
  label: string;
  /** Marks a state that is still resolving, so the footer light can breathe. */
  pulse: boolean;
  /** Whether this state earns the degraded strip. */
  degraded: boolean;
};

/**
 * How each connection state states itself.
 *
 * One table, because the footer light and the degraded strip are on screen at
 * the same moment: a second copy is how they would come to disagree about what
 * the same connection is doing.
 *
 * `degraded` is deliberately false for `Connecting`. `run_sse_loop` sets that
 * state at the top of every iteration, including the first, so treating it as
 * degraded would flash a band across a perfectly healthy cold start — on a
 * window that ADR 0002 requires to say nothing about itself when nothing is
 * wrong. A transient belongs in the pulsing light, not in a band that costs a
 * row.
 *
 * `CONNECTING` rather than `SYNCING`: `CONTEXT.md` puts "sync" on Contact's
 * _Avoid_ list, and the state's own name is already the honest word.
 */
export const CONNECTION: Record<ConnectionState, ConnectionReadout> = {
  Online: { tone: "nominal", label: "ONLINE", pulse: false, degraded: false },
  Connecting: { tone: "caution", label: "CONNECTING", pulse: true, degraded: false },
  Disconnected: { tone: "standby", label: "OFFLINE", pulse: false, degraded: true },
  AuthFailed: { tone: "alert", label: "AUTH FAILED", pulse: false, degraded: true },
};
