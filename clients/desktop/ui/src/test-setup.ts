import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// jsdom does not implement scrollIntoView; HistoryList calls it to keep the
// keyboard selection on screen. Tests that assert on it read the spy back with
// vi.mocked(Element.prototype.scrollIntoView).
Element.prototype.scrollIntoView = vi.fn() as unknown as Element["scrollIntoView"];
