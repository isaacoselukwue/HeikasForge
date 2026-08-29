import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

import { establishSession } from "@/api/client";
import { LoadingState, ErrorState } from "@/components/StateViews";
import { SessionContext } from "./sessionContext";

type Phase = "connecting" | "ready" | "failed";

function readHashParameters(): { token: string | null; run: string | null } {
  const hash = window.location.hash.replace(/^#/, "");
  if (hash.length === 0) {
    return { token: null, run: null };
  }
  const parameters = new URLSearchParams(hash);
  return { token: parameters.get("token"), run: parameters.get("run") };
}

export function SessionProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<Phase>("connecting");
  const [demonstrationMode, setDemonstrationMode] = useState(false);
  const [initialRunId, setInitialRunId] = useState<string | null>(null);
  const [failure, setFailure] = useState<string>("");

  useEffect(() => {
    let cancelled = false;
    const { token, run } = readHashParameters();
    setInitialRunId(run);
    establishSession(token)
      .then((session) => {
        if (cancelled) {
          return;
        }
        setDemonstrationMode(session.demonstration_mode);
        setPhase("ready");
        if (token !== null) {
          window.history.replaceState(null, "", window.location.pathname + window.location.search);
        }
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setFailure(error instanceof Error ? error.message : "the session could not be established");
        setPhase("failed");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const value = useMemo(
    () => ({ demonstrationMode, initialRunId }),
    [demonstrationMode, initialRunId],
  );

  if (phase === "connecting") {
    return <LoadingState label="Establishing the local session" className="h-screen" />;
  }

  if (phase === "failed") {
    return (
      <div className="mx-auto max-w-2xl p-8">
        <ErrorState
          title="The local session could not be established"
          message={failure}
          remedy="Reopen the interface from the link that `heikas ui` printed so the bootstrap token is present."
          sourceChangesPossible={false}
        />
      </div>
    );
  }

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
