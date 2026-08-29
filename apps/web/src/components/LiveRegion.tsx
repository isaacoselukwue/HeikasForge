import { useEffect, useState } from "react";

export interface LiveRegionProps {
  message: string | null;
  politeness?: "polite" | "assertive";
}

export function LiveRegion({ message, politeness = "polite" }: LiveRegionProps) {
  const [announced, setAnnounced] = useState("");

  useEffect(() => {
    if (message === null || message.length === 0) {
      return;
    }
    setAnnounced("");
    const timer = window.setTimeout(() => {
      setAnnounced(message);
    }, 40);
    return () => {
      window.clearTimeout(timer);
    };
  }, [message]);

  return (
    <div aria-live={politeness} aria-atomic="true" className="sr-only">
      {announced}
    </div>
  );
}
