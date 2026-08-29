import AxeBuilder from "@axe-core/playwright";

import { expect, test } from "./fixtures";

test.describe("the control room", () => {
  test("lists the seeded runs on the dashboard", async ({ authenticated, runtime }) => {
    await expect(authenticated.getByRole("heading", { name: "Runs", level: 1 })).toBeVisible();
    await expect(authenticated.getByRole("heading", { name: "History" })).toBeVisible();
    await expect(
      authenticated.getByText("Round invoice currency half away from zero").first(),
    ).toBeVisible();
    await expect(authenticated.getByText(runtime.repository).first()).toBeVisible();
  });

  test("filters the dashboard by search text", async ({ authenticated }) => {
    const search = authenticated.getByLabel("Search runs");
    await search.fill("no such run exists");
    await expect(authenticated.getByText("Nothing to show yet")).toBeVisible();
    await search.fill("");
    await expect(authenticated.getByText("Nothing to show yet")).toBeHidden();
  });

  test("shows the completed run with its graph, timeline and metrics", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}`);
    await expect(authenticated.getByText("Succeeded").first()).toBeVisible();
    await expect(authenticated.getByRole("group", { name: /Run graph/ })).toBeVisible();

    await authenticated.getByRole("tab", { name: "Timeline" }).click();
    await expect(authenticated.getByRole("log", { name: "Run timeline" })).toBeVisible();
    await authenticated.getByLabel("Level").selectOption("warning");
    await expect(authenticated.getByText(/repair 1 of 2 started/).first()).toBeVisible();
    await authenticated.getByLabel("Level").selectOption("all");

    await authenticated.getByRole("tab", { name: "Logs" }).click();
    await expect(authenticated.getByText(/Secrets are redacted/)).toBeVisible();

    await expect(authenticated.getByText("Node executions")).toBeVisible();
    await expect(authenticated.getByText("Repair loops")).toBeVisible();
  });

  test("filters the timeline by level and candidate", async ({ authenticated, runtime }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}`);
    await authenticated.getByRole("tab", { name: "Timeline" }).click();
    const total = await authenticated.getByText(/of \d+ events/).textContent();
    await authenticated.getByLabel("Level").selectOption("failure");
    const filtered = await authenticated.getByText(/of \d+ events/).textContent();
    expect(filtered).not.toBe(total);
    await expect(authenticated.getByRole("log", { name: "Run timeline" })).toBeVisible();
  });

  test("explains the deterministic winner on the comparison page", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}/candidates`);
    await expect(authenticated.getByRole("heading", { name: /^Winner/ })).toBeVisible();
    await expect(authenticated.getByText(/decided by Changed lines/)).toBeVisible();
    await expect(authenticated.getByRole("heading", { name: "Exclusion reasons" })).toBeVisible();
    await expect(
      authenticated.getByText(/declared 3 tests at the baseline/).first(),
    ).toBeVisible();

    const rows = authenticated.getByRole("row");
    await expect(rows).toHaveCount(4);
  });

  test("sorts the candidate table from the keyboard", async ({ authenticated, runtime }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}/candidates`);
    const heading = authenticated.getByRole("button", { name: /Changed lines/ });
    await heading.focus();
    await authenticated.keyboard.press("Enter");
    await expect(
      authenticated.getByRole("columnheader", { name: /Changed lines/ }),
    ).toHaveAttribute("aria-sort", "descending");
  });

  test("shows the plan for a run awaiting approval and never claims code changed", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.pendingRunId}/plan`);
    await expect(authenticated.getByRole("heading", { name: "Plan approval" })).toBeVisible();
    await expect(authenticated.getByText("Awaiting approval")).toBeVisible();
    await expect(authenticated.getByText(/No candidate source has been changed yet/)).toBeVisible();
    await expect(authenticated.getByRole("heading", { name: "Task interpretation" })).toBeVisible();
    await expect(authenticated.getByText("src/invoice.py").first()).toBeVisible();
  });

  test("editing the plan invalidates the approval and records a new version", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.pendingRunId}/plan`);
    await authenticated.getByRole("button", { name: "Edit" }).click();
    const editor = authenticated.locator(".cm-content");
    await expect(editor).toBeVisible();
    await editor.click();
    await authenticated.keyboard.press("Control+End");
    await authenticated.keyboard.type("\nAn operator clarification.\n");
    await authenticated.getByRole("button", { name: "Save as a new version" }).click();
    await expect(authenticated.getByText("Version 2")).toBeVisible();
    await expect(authenticated.getByText("Edited by you")).toBeVisible();
    await expect(authenticated.getByText("Awaiting approval")).toBeVisible();
  });

  test("the doctor reports the environment and the free local path", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/doctor`);
    await authenticated.getByLabel("Repository path").fill(runtime.repository);
    await authenticated.getByRole("button", { name: "Run the diagnosis" }).click();
    await expect(
      authenticated.getByRole("heading", { name: "Adapter matrix" }),
    ).toBeVisible({ timeout: 120_000 });
    await expect(authenticated.getByText("Host platform")).toBeVisible();
    await expect(authenticated.getByText("Repository", { exact: true })).toBeVisible();
  });

  test("settings expose the data location and the privacy position", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/settings`);
    await expect(
      authenticated.getByText(runtime.heikasHome, { exact: true }),
    ).toBeVisible();
    await expect(authenticated.getByText(/No telemetry leaves this machine/)).toBeVisible();
  });

  test("the interface is reachable by keyboard alone", async ({ authenticated }) => {
    await authenticated.keyboard.press("Tab");
    await expect(
      authenticated.getByRole("link", { name: "Skip to the main content" }),
    ).toBeFocused();
    await authenticated.keyboard.press("Enter");
    await expect(authenticated.locator("#main-content")).toBeVisible();
  });

  test("the command palette opens from the keyboard and navigates", async ({ authenticated }) => {
    await authenticated.keyboard.press("Control+k");
    const search = authenticated.getByRole("textbox", { name: "Search actions" });
    await expect(search).toBeVisible();
    await search.fill("doctor");
    await authenticated.keyboard.press("Enter");
    await expect(authenticated.getByRole("heading", { name: "Doctor", level: 1 })).toBeVisible();
  });

  test("the theme switch changes the document theme", async ({ authenticated }) => {
    const root = authenticated.locator("html");
    await expect(root).toHaveAttribute("data-theme", "dark");
    await authenticated.getByRole("button", { name: "Switch to the light theme" }).click();
    await expect(root).toHaveAttribute("data-theme", "light");
  });

  test("a missing route explains itself rather than failing silently", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/no-such-route`);
    await expect(authenticated.getByText("That route does not exist")).toBeVisible();
  });

  test("the dashboard has no serious accessibility violations", async ({ authenticated }) => {
    const results = await new AxeBuilder({ page: authenticated })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
      .analyze();
    expect(results.violations.map((violation) => `${violation.id}: ${violation.help}`)).toEqual([]);
  });

  test("the run detail has no serious accessibility violations", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}`);
    await expect(authenticated.getByRole("group", { name: /Run graph/ })).toBeVisible();
    const results = await new AxeBuilder({ page: authenticated })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
      .analyze();
    expect(results.violations.map((violation) => `${violation.id}: ${violation.help}`)).toEqual([]);
  });

  test("the candidate comparison has no serious accessibility violations", async ({
    authenticated,
    runtime,
  }) => {
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}/candidates`);
    await expect(authenticated.getByRole("heading", { name: /^Winner/ })).toBeVisible();
    const results = await new AxeBuilder({ page: authenticated })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
      .analyze();
    expect(results.violations.map((violation) => `${violation.id}: ${violation.help}`)).toEqual([]);
  });

  test("the interface makes no third party request", async ({ authenticated, runtime }) => {
    const foreign: string[] = [];
    authenticated.on("request", (request) => {
      if (!request.url().startsWith(runtime.baseUrl) && !request.url().startsWith("data:")) {
        foreign.push(request.url());
      }
    });
    await authenticated.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}`);
    await authenticated.getByRole("tab", { name: "Timeline" }).click();
    await authenticated.waitForTimeout(1_500);
    expect(foreign).toEqual([]);
  });
});
