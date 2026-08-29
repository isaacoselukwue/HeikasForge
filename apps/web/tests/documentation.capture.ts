import { mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";
import { workspaceRoot } from "./orchestrator";

const mediaDirectory = join(workspaceRoot, "docs", "media");
const framesDirectory = join(mediaDirectory, "frames");

test("captures the documentation media from the running application", async ({
  page,
  context,
  runtime,
}) => {
  mkdirSync(mediaDirectory, { recursive: true });
  rmSync(framesDirectory, { recursive: true, force: true });
  mkdirSync(framesDirectory, { recursive: true });

  let frameIndex = 0;
  const captureFrame = async (): Promise<void> => {
    const name = String(frameIndex).padStart(4, "0");
    frameIndex += 1;
    await page.screenshot({ path: join(framesDirectory, `${name}.png`) });
  };
  const hold = async (frames: number, delay = 220): Promise<void> => {
    for (let index = 0; index < frames; index += 1) {
      await captureFrame();
      await page.waitForTimeout(delay);
    }
  };

  const bootstrap = new URL(runtime.bootstrapUrl);
  await page.goto(`${runtime.baseUrl}/${bootstrap.hash}`);
  await expect(page.getByRole("navigation", { name: "Primary" })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("heading", { name: "Runs", level: 1 })).toBeVisible();
  await page.waitForTimeout(600);
  await page.screenshot({ path: join(mediaDirectory, "dashboard.png") });
  await hold(6);

  await page.goto(`${runtime.baseUrl}/runs/${runtime.pendingRunId}/plan`);
  await expect(page.getByRole("heading", { name: "Plan approval" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Task interpretation" })).toBeVisible();
  await page.waitForTimeout(600);
  await page.screenshot({ path: join(mediaDirectory, "plan-approval.png") });
  await hold(8);

  await page.getByRole("button", { name: "Approve and start candidates" }).click();
  await expect(page).toHaveURL(new RegExp(`/runs/${runtime.pendingRunId}$`), { timeout: 60_000 });
  await hold(26, 320);

  await page.getByRole("tab", { name: "Timeline" }).click();
  await hold(8, 260);

  await page.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}`);
  await expect(page.getByRole("group", { name: /Run graph/ })).toBeVisible();
  await page.waitForTimeout(900);
  await page.screenshot({ path: join(mediaDirectory, "run-detail.png") });
  await hold(8);

  await page.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}/candidates`);
  await expect(page.getByRole("heading", { name: /^Winner/ })).toBeVisible();
  await page.waitForTimeout(700);
  await page.screenshot({ path: join(mediaDirectory, "candidate-comparison.png") });
  await hold(10);

  await page.goto(`${runtime.baseUrl}/runs/${runtime.completedRunId}`);
  await page.getByRole("tab", { name: "Timeline" }).click();
  await expect(page.getByRole("log", { name: "Run timeline" })).toBeVisible();
  await hold(8, 260);

  const video = page.video();
  await context.close();
  if (video) {
    await video.saveAs(join(mediaDirectory, "demo.webm"));
  }
});
