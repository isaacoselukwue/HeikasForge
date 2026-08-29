import { useRef } from "react";
import type { ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

export interface VirtualListProps<T> {
  items: T[];
  estimateSize: number;
  label: string;
  renderItem: (item: T, index: number) => ReactNode;
  emptyState: ReactNode;
  className?: string;
}

export function VirtualList<T>({
  items,
  estimateSize,
  label,
  renderItem,
  emptyState,
  className,
}: VirtualListProps<T>) {
  const container = useRef<HTMLDivElement | null>(null);
  const virtualiser = useVirtualizer({
    count: items.length,
    getScrollElement: () => container.current,
    estimateSize: () => estimateSize,
    overscan: 12,
  });

  if (items.length === 0) {
    return <div className={className}>{emptyState}</div>;
  }

  return (
    <div
      ref={container}
      role="log"
      aria-label={label}
      className={`scrollbar-slim overflow-auto ${className ?? ""}`}
    >
      <div style={{ height: `${String(virtualiser.getTotalSize())}px`, position: "relative" }}>
        {virtualiser.getVirtualItems().map((virtualRow) => {
          const item = items[virtualRow.index];
          if (item === undefined) {
            return null;
          }
          return (
            <div
              key={virtualRow.key}
              data-index={virtualRow.index}
              ref={virtualiser.measureElement}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${String(virtualRow.start)}px)`,
              }}
            >
              {renderItem(item, virtualRow.index)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
