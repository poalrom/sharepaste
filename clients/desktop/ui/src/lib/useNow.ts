import { useEffect, useState } from "react";

/**
 * A clock that only runs while the window has focus.
 *
 * Relative ages have to tick, but the popover spends almost all of its life
 * hidden — and a hidden window is still mounted, because `popover.rs` shows and
 * hides it rather than unmounting. Ticking only while focused keeps a tray app
 * from waking the CPU once a minute forever, and the `focus` listener makes the
 * first render after a show correct rather than up to `intervalMs` stale.
 */
export function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let timer: number | undefined;

    const stop = () => {
      if (timer !== undefined) {
        window.clearInterval(timer);
        timer = undefined;
      }
    };

    const start = () => {
      setNow(Date.now());
      if (timer === undefined) timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    };

    if (document.hasFocus()) start();
    window.addEventListener("focus", start);
    window.addEventListener("blur", stop);
    return () => {
      stop();
      window.removeEventListener("focus", start);
      window.removeEventListener("blur", stop);
    };
  }, [intervalMs]);

  return now;
}
