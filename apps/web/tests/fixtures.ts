import { test as base, expect } from "@playwright/test";
import type { Page } from "@playwright/test";

import { readRuntime } from "./orchestrator";
import type { RuntimeDescriptor } from "./orchestrator";

export interface ForgeFixtures {
  runtime: RuntimeDescriptor;
  authenticated: Page;
}

export const test = base.extend<ForgeFixtures>({
  runtime: async ({}, use) => {
    await use(readRuntime());
  },
  authenticated: async ({ page, runtime }, use) => {
    const bootstrap = new URL(runtime.bootstrapUrl);
    await page.goto(`${runtime.baseUrl}/${bootstrap.hash}`);
    await expect(page.getByRole("navigation", { name: "Primary" })).toBeVisible({
      timeout: 30_000,
    });
    await use(page);
  },
});

export { expect };
