import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
});

if (!("matchMedia" in window)) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });
}

class ResizeObserverStub {
  observe(): void {
    return;
  }
  unobserve(): void {
    return;
  }
  disconnect(): void {
    return;
  }
}

globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;

if (!("EventSource" in globalThis)) {
  class EventSourceStub {
    onopen: (() => void) | null = null;
    onmessage: (() => void) | null = null;
    onerror: (() => void) | null = null;
    close(): void {
      return;
    }
  }
  globalThis.EventSource = EventSourceStub as unknown as typeof EventSource;
}
