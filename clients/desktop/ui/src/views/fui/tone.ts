/**
 * The signal vocabulary the whole shell shares.
 *
 * Every `fui/` component takes one of these rather than a colour, so a state's
 * meaning is chosen once and its palette is chosen in `styles.css`.
 */
export type Tone = "nominal" | "caution" | "alert" | "standby" | "cyan";
