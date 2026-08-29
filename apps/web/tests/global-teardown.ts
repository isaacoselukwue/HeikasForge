import { stopOrchestrator } from "./orchestrator";

export default function globalTeardown(): void {
  stopOrchestrator();
}
