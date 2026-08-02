import { describe, it, expect } from "vitest";
import { SseHub, type SseEvent } from "../../src/server/sse-hub.js";

describe("SseHub", () => {
  it("delivers events only to subscribers of the matching user", () => {
    const hub = new SseHub();
    const aReceived: unknown[] = [];
    const bReceived: unknown[] = [];

    const unsubA = hub.subscribe("user-a", (e) => aReceived.push(e));
    const unsubB = hub.subscribe("user-b", (e) => bReceived.push(e));

    const first: SseEvent = {
      type: "entry",
      id: 1,
      ciphertext: "AAAA",
      created_at: 1,
      device_id: "d1",
      seq: 1,
      last_use: 1,
    };
    hub.publish("user-a", first);
    hub.publish("user-b", { type: "delete", id: 7 });

    expect(aReceived).toEqual([first]);
    expect(bReceived).toEqual([{ type: "delete", id: 7 }]);

    unsubA();
    hub.publish("user-a", {
      type: "entry",
      id: 2,
      ciphertext: "BBBB",
      created_at: 2,
      device_id: "d2",
      seq: 2,
      last_use: 2,
    });
    expect(aReceived).toEqual([first]);
    unsubB();
  });

  it("supports multiple subscribers per user", () => {
    const hub = new SseHub();
    const r1: unknown[] = [];
    const r2: unknown[] = [];
    hub.subscribe("user-a", (e) => r1.push(e));
    hub.subscribe("user-a", (e) => r2.push(e));
    const event: SseEvent = {
      type: "entry",
      id: 5,
      ciphertext: "AAAA",
      created_at: 5,
      device_id: "d1",
      seq: 5,
      last_use: 5,
    };
    hub.publish("user-a", event);
    expect(r1).toEqual([event]);
    expect(r2).toEqual([event]);
  });
});
