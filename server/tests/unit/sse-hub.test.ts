import { describe, it, expect } from "vitest";
import { SseHub, type SseEvent } from "../../src/server/sse-hub.js";

// NOTE: Tests publish minimal "entry" event literals that omit ciphertext/created_at/device_id
// (which the strict SseEvent shape requires). To keep the test bodies readable and aligned with
// the spec, we cast these literals to `SseEvent` via `as unknown as SseEvent`. The real shape is
// enforced everywhere else publish() is called from production code.
describe("SseHub", () => {
  it("delivers events only to subscribers of the matching user", () => {
    const hub = new SseHub();
    const aReceived: unknown[] = [];
    const bReceived: unknown[] = [];

    const unsubA = hub.subscribe("user-a", (e) => aReceived.push(e));
    const unsubB = hub.subscribe("user-b", (e) => bReceived.push(e));

    hub.publish("user-a", { type: "entry", id: 1 } as unknown as SseEvent);
    hub.publish("user-b", { type: "delete", id: 7 });

    expect(aReceived).toEqual([{ type: "entry", id: 1 }]);
    expect(bReceived).toEqual([{ type: "delete", id: 7 }]);

    unsubA();
    hub.publish("user-a", { type: "entry", id: 2 } as unknown as SseEvent);
    expect(aReceived).toEqual([{ type: "entry", id: 1 }]);
    unsubB();
  });

  it("supports multiple subscribers per user", () => {
    const hub = new SseHub();
    const r1: unknown[] = [];
    const r2: unknown[] = [];
    hub.subscribe("user-a", (e) => r1.push(e));
    hub.subscribe("user-a", (e) => r2.push(e));
    hub.publish("user-a", { type: "entry", id: 5 } as unknown as SseEvent);
    expect(r1).toEqual([{ type: "entry", id: 5 }]);
    expect(r2).toEqual([{ type: "entry", id: 5 }]);
  });
});
