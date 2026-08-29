import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { eventStreamUrl } from "./client";
import { queryKeys } from "./queries";
import type { DurableEvent } from "@/generated/api-types";

export type StreamState = "connecting" | "live" | "reconnecting" | "closed";

export interface LiveStream {
  state: StreamState;
  lastSequence: number;
  lastEvent: DurableEvent | null;
  eventsReceived: number;
}

const REFRESH_DEBOUNCE_MILLISECONDS = 250;

export function useRunEventStream(runId: string, initialSequence: number): LiveStream {
  const client = useQueryClient();
  const [state, setState] = useState<StreamState>("connecting");
  const [lastSequence, setLastSequence] = useState(initialSequence);
  const [lastEvent, setLastEvent] = useState<DurableEvent | null>(null);
  const [eventsReceived, setEventsReceived] = useState(0);
  const sequenceRef = useRef(initialSequence);
  const refreshTimer = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let source: EventSource | null = null;
    let reconnectTimer: number | null = null;

    const scheduleRefresh = () => {
      if (refreshTimer.current !== null) {
        window.clearTimeout(refreshTimer.current);
      }
      refreshTimer.current = window.setTimeout(() => {
        void client.invalidateQueries({ queryKey: queryKeys.run(runId) });
        void client.invalidateQueries({ queryKey: queryKeys.candidates(runId) });
        void client.invalidateQueries({ queryKey: queryKeys.timeline(runId) });
        void client.invalidateQueries({ queryKey: queryKeys.plan(runId) });
        void client.invalidateQueries({ queryKey: queryKeys.runs });
      }, REFRESH_DEBOUNCE_MILLISECONDS);
    };

    const connect = () => {
      if (cancelled) {
        return;
      }
      source = new EventSource(eventStreamUrl(runId, sequenceRef.current));
      source.onopen = () => {
        if (!cancelled) {
          setState("live");
        }
      };
      source.onmessage = (message: MessageEvent<string>) => {
        try {
          const event = JSON.parse(message.data) as DurableEvent;
          sequenceRef.current = Math.max(sequenceRef.current, event.sequence);
          setLastSequence(sequenceRef.current);
          setLastEvent(event);
          setEventsReceived((count) => count + 1);
          scheduleRefresh();
        } catch {
          scheduleRefresh();
        }
      };
      source.onerror = () => {
        source?.close();
        if (cancelled) {
          return;
        }
        setState("reconnecting");
        reconnectTimer = window.setTimeout(connect, 1_500);
      };
    };

    connect();

    return () => {
      cancelled = true;
      setState("closed");
      source?.close();
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
      }
      if (refreshTimer.current !== null) {
        window.clearTimeout(refreshTimer.current);
      }
    };
  }, [client, runId]);

  return { state, lastSequence, lastEvent, eventsReceived };
}
