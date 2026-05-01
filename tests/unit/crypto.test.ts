import { describe, it, expect } from "vitest";
import {
  randomId,
  randomToken,
  sha256Hex,
  hashToken,
  verifyToken,
  timingSafeEqualHex,
} from "../../src/crypto.js";

describe("randomId", () => {
  it("returns a unique string each call", () => {
    const a = randomId();
    const b = randomId();
    expect(a).not.toBe(b);
    expect(a).toMatch(/^[0-9a-f-]{36}$/);
  });
});

describe("randomToken", () => {
  it("returns 43-char base64url string for 32 random bytes", () => {
    const t = randomToken();
    expect(t).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(t).not.toBe(randomToken());
  });
});

describe("sha256Hex", () => {
  it("matches a known vector", () => {
    expect(sha256Hex("abc")).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
  });
});

describe("hashToken / verifyToken", () => {
  it("round-trips an argon2id hash", async () => {
    const t = randomToken();
    const h = await hashToken(t);
    expect(h).not.toContain(t);
    expect(await verifyToken(h, t)).toBe(true);
    expect(await verifyToken(h, "wrong-" + t)).toBe(false);
  });
});

describe("timingSafeEqualHex", () => {
  it("returns true for equal hex strings of any case and false otherwise", () => {
    expect(timingSafeEqualHex("ab", "ab")).toBe(true);
    expect(timingSafeEqualHex("ab", "AB")).toBe(true);
    expect(timingSafeEqualHex("ab", "ac")).toBe(false);
    expect(timingSafeEqualHex("ab", "abc")).toBe(false);
  });
});
