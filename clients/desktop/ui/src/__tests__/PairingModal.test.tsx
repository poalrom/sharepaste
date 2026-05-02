import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { injectForTests } from "../ipc/tauri";
import PairingModal from "../modals/PairingModal";

beforeEach(() => {
  const invoke = vi.fn(async () => ({ user_id: "u", device_id: "d" }));
  const listen = vi.fn(async () => () => {});
  injectForTests(invoke as never, listen as never);
});

describe("PairingModal", () => {
  it("starts on the chooser screen", () => {
    render(<PairingModal />);
    expect(screen.getByTestId("choose-invite")).toBeInTheDocument();
    expect(screen.getByTestId("choose-code")).toBeInTheDocument();
  });

  it("navigates to the invite step", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    expect(screen.getByText(/Claim invite/i)).toBeInTheDocument();
  });

  it("warns on plain http to non-localhost", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-invite"));
    const url = screen.getByLabelText(/Server URL/i, { selector: "input" }) as HTMLInputElement;
    fireEvent.change(url, { target: { value: "http://example.com" } });
    expect(screen.getByTestId("insecure-warning")).toBeInTheDocument();
  });

  it("shows red border on invalid pair code", () => {
    render(<PairingModal />);
    fireEvent.click(screen.getByTestId("choose-code"));
    const ta = screen.getByTestId("pair-code") as HTMLTextAreaElement;
    fireEvent.change(ta, { target: { value: "tiny" } });
    expect(ta.className).toContain("ring-red-500");
  });
});
