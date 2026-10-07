import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { ErrorPanel } from "./ErrorPanel";

describe("ErrorPanel", () => {
  it("shows the headline and toggles technical details", async () => {
    render(
      <ErrorPanel
        error={{
          kind: "connectionRefused",
          message: "Unable to connect to Android device.",
          details: "ADB returned:\ndevice offline",
        }}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Unable to connect to Android device.");
    expect(screen.queryByText(/device offline/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/device offline/)).toBeInTheDocument();
  });

  it("hides the details toggle when there are none", () => {
    render(<ErrorPanel error={{ kind: "io", message: "boom", details: null }} />);
    expect(screen.queryByRole("button", { name: "Details" })).not.toBeInTheDocument();
  });
});

describe("ConfirmDialog", () => {
  it("only confirms on the confirm button", async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Stop Hermes?"
        confirmLabel="Stop"
        danger
        onConfirm={onConfirm}
        onCancel={onCancel}
      >
        Hermes will stop processing messages.
      </ConfirmDialog>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("renders nothing when closed", () => {
    render(
      <ConfirmDialog open={false} title="x" onConfirm={() => {}} onCancel={() => {}}>
        body
      </ConfirmDialog>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("Dialog keyboard navigation", () => {
  it("keeps forward and reverse Tab focus inside the open dialog", () => {
    render(
      <Dialog open title="Keyboard dialog" onClose={() => {}}>
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </Dialog>,
    );
    const first = screen.getByRole("button", { name: "First action" });
    const last = screen.getByRole("button", { name: "Last action" });

    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(first).toHaveFocus();

    first.focus();
    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(last).toHaveFocus();
  });
});
