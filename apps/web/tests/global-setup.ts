import { startOrchestrator } from "./orchestrator";

export default async function globalSetup(): Promise<void> {
  const runtime = await startOrchestrator();
  console.log(`the local interface is serving ${runtime.baseUrl}`);
}
