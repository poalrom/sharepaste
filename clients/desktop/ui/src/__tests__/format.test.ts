import { describe, expect, it } from "vitest";
import { agePhrase, normalizePreview, originLabel, relativeAge } from "../lib/format";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("relativeAge", () => {
  it("reads as 'now' below a minute, so a fresh entry never claims an age", () => {
    expect(relativeAge(1_000_000, 1_000_000)).toBe("now");
    expect(relativeAge(1_000_000, 1_000_000 + 59_999)).toBe("now");
  });

  it("steps to minutes, hours, days and months at each boundary", () => {
    const t = 1_000_000_000;
    expect(relativeAge(t, t + MINUTE)).toBe("1m");
    expect(relativeAge(t, t + 59 * MINUTE)).toBe("59m");
    expect(relativeAge(t, t + HOUR)).toBe("1h");
    expect(relativeAge(t, t + 23 * HOUR)).toBe("23h");
    expect(relativeAge(t, t + DAY)).toBe("1d");
    expect(relativeAge(t, t + 29 * DAY)).toBe("29d");
    expect(relativeAge(t, t + 30 * DAY)).toBe("1mo");
  });

  it("truncates rather than rounds, so a label never overstates the age", () => {
    const t = 1_000_000_000;
    expect(relativeAge(t, t + 119_999)).toBe("1m");
    expect(relativeAge(t, t + 2 * HOUR - 1)).toBe("1h");
  });

  it("clamps a future timestamp to 'now' rather than emitting a negative age", () => {
    const t = 1_000_000_000;
    expect(relativeAge(t, t - 5 * HOUR)).toBe("now");
  });
});

describe("agePhrase", () => {
  // The bug this exists to prevent: four surfaces each appended "ago" to
  // relativeAge, and every one of them rendered "now ago" under a minute.
  it("leaves the sub-minute reading alone rather than saying 'now ago'", () => {
    const t = 1_000_000_000;
    expect(agePhrase(t, t)).toBe("now");
    expect(agePhrase(t, t + 59_999)).toBe("now");
    expect(agePhrase(t, t - HOUR)).toBe("now");
  });

  it("suffixes every real age", () => {
    const t = 1_000_000_000;
    expect(agePhrase(t, t + MINUTE)).toBe("1m ago");
    expect(agePhrase(t, t + 4 * HOUR)).toBe("4h ago");
    expect(agePhrase(t, t + 30 * DAY)).toBe("1mo ago");
  });
});

describe("normalizePreview", () => {
  it("collapses every run of whitespace to a single space", () => {
    expect(normalizePreview("npm   run\tdev")).toBe("npm run dev");
  });

  it("flattens a multi-line entry so the row is not visually empty", () => {
    expect(normalizePreview("\n\n    const x = 1;\n    const y = 2;\n")).toBe(
      "const x = 1; const y = 2;",
    );
  });

  it("trims leading indentation that would otherwise render as a blank row", () => {
    expect(normalizePreview("        indented")).toBe("indented");
  });

  it("bounds the string so an unbounded plaintext cannot enter the DOM whole", () => {
    expect(normalizePreview("x".repeat(500))).toHaveLength(200);
    expect(normalizePreview("x".repeat(500), 400)).toHaveLength(400);
  });

  it("leaves a short string untouched", () => {
    expect(normalizePreview("ss://Y2hhY2hh")).toBe("ss://Y2hhY2hh");
  });

  it("returns an empty string for whitespace-only plaintext", () => {
    expect(normalizePreview("   \n\t ")).toBe("");
  });
});

describe("originLabel", () => {
  it("prefers the mirrored Device Label", () => {
    expect(originLabel("iphone-15", "abcdef123456")).toBe("iphone-15");
  });

  it("falls back to a 4-char device id slice for an unlabelled legacy membership", () => {
    expect(originLabel(null, "abcdef123456")).toBe("abcd");
    expect(originLabel(undefined, "abcdef123456")).toBe("abcd");
  });

  it("treats a blank label as unlabelled", () => {
    expect(originLabel("   ", "abcdef123456")).toBe("abcd");
  });
});
