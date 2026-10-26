// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { HandoffActions } from "./HandoffActions";

describe("HandoffActions", () => {
  beforeEach(() => setLanguage("es"));
  afterEach(cleanup);

  it("shows the start without buttons", () => {
    const { getByText, queryAllByRole } = render(
      <HandoffActions task="A-1" handoff={{ to: "review", stage: "starting" }} />,
    );
    expect(getByText("Empezando la sesión nueva…")).toBeTruthy();
    expect(queryAllByRole("button")).toHaveLength(0);
  });
});

describe("HandoffActions offer", () => {
  beforeEach(() => setLanguage("en"));
  afterEach(cleanup);

  it("uses the kit buttons: primary to implement, secondary to stay", () => {
    const { getByText } = render(<HandoffActions task="A-1" handoff={{ to: "review", stage: "offered" }} />);
    expect(getByText("Implement in a new session").closest("button")!.className).toContain("b-p");
    expect(getByText("Keep going here").closest("button")!.className).toContain("b-s");
  });
});
