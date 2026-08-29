import { createContext, useContext } from "react";

export interface SessionContextValue {
  demonstrationMode: boolean;
  initialRunId: string | null;
}

export const SessionContext = createContext<SessionContextValue>({
  demonstrationMode: false,
  initialRunId: null,
});

export function useSession(): SessionContextValue {
  return useContext(SessionContext);
}
