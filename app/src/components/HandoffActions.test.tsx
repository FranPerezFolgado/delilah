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
