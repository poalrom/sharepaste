export type SseEvent =
  | {
      type: "entry";
      id: number;
      ciphertext: string;
      created_at: number;
      device_id: string;
      seq: number;
      last_use: number;
    }
  | { type: "delete"; id: number };

export type SseListener = (event: SseEvent) => void;

export class SseHub {
  private readonly subscribers = new Map<string, Set<SseListener>>();

  subscribe(userId: string, listener: SseListener): () => void {
    let set = this.subscribers.get(userId);
    if (!set) {
      set = new Set();
      this.subscribers.set(userId, set);
    }
    set.add(listener);
    return () => {
      const s = this.subscribers.get(userId);
      if (!s) return;
      s.delete(listener);
      if (s.size === 0) this.subscribers.delete(userId);
    };
  }

  publish(userId: string, event: SseEvent): void {
    const s = this.subscribers.get(userId);
    if (!s) return;
    for (const fn of s) fn(event);
  }
}
