import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { device } from "../../test/fixtures";
import { mockIpc, resetStores } from "../../test/ipcMock";
import type { TermuxCheck } from "../../types";
import { setupSteps, TermuxSetupCard } from "./TermuxSetupCard";

const KEY = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITEST hermes-control-center";

describe("setupSteps", () => {
  it("embeds the public key safely and binds sshd to localhost", () => {
    const steps = setupSteps(KEY);
    expect(steps[1]?.command).toContain(`'${KEY}'`);
    expect(steps[1]?.command).toContain("grep -qxF");
    expect(steps[2]?.command).toContain("ListenAddress 127.0.0.1");
    expect(setupSteps("it's")[1]?.command).toContain(`'it'\\''s'`);
  });
});

describe("TermuxSetupCard", () => {
  beforeEach(() => resetStores());

  it("shows setup commands and a failing checklist with hints", async () => {
    const result: TermuxCheck = {
      ready: false,
      distros: [],
      items: [
        {
          id: "termux",
          label: "Termux installed",
          status: "ok",
          detail: "0.119.0-beta.3",
          hint: null,
        },
        {
          id: "forward",
          label: "Port forward",
          status: "ok",
          detail: "localhost:41234 → phone 127.0.0.1:8022",
          hint: null,
        },
        {
          id: "ssh",
          label: "SSH into Termux",
          status: "failed",
          detail: "Could not reach sshd in Termux.",
          hint: "In Termux: pkg install openssh",
        },
        { id: "prefix", label: "Termux environment", status: "skipped", detail: null, hint: null },
      ],
    };
    const fn = mockIpc({ get_termux_public_key: () => KEY, check_termux: () => result });
    render(<TermuxSetupCard device={device()} />);
    expect(await screen.findByText("pkg install -y openssh")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Verify" }));
    expect(fn).toHaveBeenCalledWith("check_termux", { serial: device().serial });
    expect(await screen.findByText("SSH into Termux: Failed")).toBeInTheDocument();
    expect(screen.getByText("In Termux: pkg install openssh")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Forget pinned host key/ })).toBeInTheDocument();
  });

  it("shows ready state with detected distros", async () => {
    mockIpc({
      get_termux_public_key: () => KEY,
      check_termux: () => ({
        ready: true,
        distros: ["debian"],
        items: [{ id: "proot", label: "proot-distro", status: "ok", detail: "debian", hint: null }],
      }),
    });
    render(<TermuxSetupCard device={device()} />);
    await userEvent.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText(/Ready — commands can run inside Termux/)).toBeInTheDocument();
    expect(screen.queryByText("pkg install -y openssh")).not.toBeInTheDocument();
  });
});
