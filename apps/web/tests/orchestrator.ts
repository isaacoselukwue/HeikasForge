import { spawn, spawnSync } from "node:child_process";
import type { ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const workspaceRoot = resolve(here, "..", "..", "..");
export const runtimeFile = join(here, ".runtime.json");

export interface RuntimeDescriptor {
  baseUrl: string;
  bootstrapUrl: string;
  completedRunId: string;
  pendingRunId: string;
  heikasHome: string;
  repository: string;
  serverPid: number;
}

function run(command: string, args: string[], environment: NodeJS.ProcessEnv = {}): string {
  const outcome = spawnSync(command, args, {
    cwd: workspaceRoot,
    encoding: "utf8",
    env: { ...process.env, ...environment },
    maxBuffer: 64 * 1024 * 1024,
  });
  if (outcome.status !== 0 && outcome.status !== 3) {
    throw new Error(
      `${command} ${args.join(" ")} failed with status ${String(outcome.status)}\n${outcome.stdout}\n${outcome.stderr}`,
    );
  }
  return outcome.stdout;
}

export function heikasExecutable(): string {
  return join(
    workspaceRoot,
    "target",
    "debug",
    process.platform === "win32" ? "heikas.exe" : "heikas",
  );
}

export async function startOrchestrator(): Promise<RuntimeDescriptor> {
  console.log("building the interface bundle");
  run("pnpm", ["--dir", "apps/web", "run", "build"]);
  console.log("building the orchestrator");
  run("cargo", ["build", "-p", "heikas-cli", "-p", "xtask"]);

  console.log("seeding the deterministic demonstration");
  const demonstration = JSON.parse(
    run("cargo", ["run", "-q", "-p", "xtask", "--", "demo", "--json"]),
  ) as { run_id: string; heikas_home: string; repository: string };

  const environment = { HEIKAS_HOME: demonstration.heikas_home };
  console.log("creating a run paused at plan approval");
  const pending = JSON.parse(
    run(
      heikasExecutable(),
      [
        "--json",
        "run",
        "--repo",
        demonstration.repository,
        "--task-file",
        join(demonstration.repository, "TASK.md"),
        "--demonstration",
        "--agent",
        "fake",
      ],
      environment,
    ),
  ) as { run_id: string };

  console.log("starting the local interface");
  const server = spawn(heikasExecutable(), ["--json", "ui", "--no-open", "--demonstration"], {
    cwd: workspaceRoot,
    env: { ...process.env, ...environment },
  }) as ChildProcessWithoutNullStreams;

  const descriptor = await readServerDescriptor(server);
  const runtime: RuntimeDescriptor = {
    baseUrl: `http://${descriptor.address}`,
    bootstrapUrl: descriptor.bootstrap_url,
    completedRunId: demonstration.run_id,
    pendingRunId: pending.run_id,
    heikasHome: demonstration.heikas_home,
    repository: demonstration.repository,
    serverPid: server.pid ?? 0,
  };
  mkdirSync(dirname(runtimeFile), { recursive: true });
  writeFileSync(runtimeFile, JSON.stringify(runtime, null, 2), "utf8");
  server.unref();
  await waitForHealth(runtime.baseUrl);
  return runtime;
}

interface ServerDescriptor {
  address: string;
  bootstrap_url: string;
}

function readServerDescriptor(server: ChildProcessWithoutNullStreams): Promise<ServerDescriptor> {
  return new Promise((resolvePromise, rejectPromise) => {
    let buffer = "";
    const timer = setTimeout(() => {
      rejectPromise(new Error(`the interface did not report its address\n${buffer}`));
    }, 60_000);
    server.stdout.setEncoding("utf8");
    server.stdout.on("data", (chunk: string) => {
      buffer += chunk;
      const start = buffer.indexOf("{");
      const end = buffer.lastIndexOf("}");
      if (start === -1 || end <= start) {
        return;
      }
      try {
        const parsed = JSON.parse(buffer.slice(start, end + 1)) as ServerDescriptor;
        if (typeof parsed.address === "string" && typeof parsed.bootstrap_url === "string") {
          clearTimeout(timer);
          resolvePromise(parsed);
        }
      } catch {
        return;
      }
    });
    server.on("exit", (code) => {
      clearTimeout(timer);
      rejectPromise(new Error(`the interface exited early with code ${String(code)}\n${buffer}`));
    });
  });
}

async function waitForHealth(baseUrl: string): Promise<void> {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/api/v1/health`);
      if (response.ok) {
        return;
      }
    } catch {
      await new Promise((wait) => setTimeout(wait, 250));
      continue;
    }
    await new Promise((wait) => setTimeout(wait, 250));
  }
  throw new Error("the local interface never became healthy");
}

export function readRuntime(): RuntimeDescriptor {
  return JSON.parse(readFileSync(runtimeFile, "utf8")) as RuntimeDescriptor;
}

export function stopOrchestrator(): void {
  try {
    const runtime = readRuntime();
    if (runtime.serverPid > 0) {
      process.kill(runtime.serverPid, "SIGTERM");
    }
  } catch {
    return;
  }
}
