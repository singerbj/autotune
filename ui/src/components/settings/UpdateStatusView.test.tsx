import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { UpdateStatus } from "@/bindings";

import { UpdateStatusView } from "./UpdateStatusView";

describe("UpdateStatusView (FR-22)", () => {
  const cases: Array<[UpdateStatus, RegExp]> = [
    [{ state: "idle" }, /haven't been checked/],
    [{ state: "checking" }, /Checking for updates/],
    [{ state: "upToDate", current: "1.2.3" }, /up to date \(version 1\.2\.3\)/],
    [
      { state: "downloading", version: "1.3.0", percent: 42.4 },
      /Downloading version 1\.3\.0 — 42%/,
    ],
    [{ state: "downloading", version: "1.3.0", percent: null }, /Downloading version 1\.3\.0…/],
    [{ state: "ready", version: "1.3.0", notes: "Faster tuning" }, /Version 1\.3\.0 is ready/],
    [{ state: "error", message: "Network down" }, /Update failed: Network down/],
  ];

  it.each(cases)("renders %o", (status, text) => {
    render(<UpdateStatusView status={status} />);
    expect(screen.getByText(text)).toBeInTheDocument();
  });

  it("shows release notes and download progress", () => {
    const { rerender } = render(
      <UpdateStatusView status={{ state: "ready", version: "2.0.0", notes: "Faster tuning" }} />,
    );
    expect(screen.getByText("Faster tuning")).toBeInTheDocument();
    rerender(<UpdateStatusView status={{ state: "downloading", version: "2.0.0", percent: 60 }} />);
    expect(screen.getByRole("progressbar", { name: "Update download" })).toHaveAttribute(
      "aria-valuenow",
      "60",
    );
  });
});
