import { useEffect, useMemo, useRef, useState } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Search } from "lucide-react";

export interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  group: string;
  perform: () => void;
}

export interface CommandPaletteProps {
  commands: PaletteCommand[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CommandPalette({ commands, open, onOpenChange }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (needle.length === 0) {
      return commands;
    }
    return commands.filter(
      (command) =>
        command.label.toLowerCase().includes(needle) ||
        command.group.toLowerCase().includes(needle) ||
        (command.hint ?? "").toLowerCase().includes(needle),
    );
  }, [commands, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query, open]);

  useEffect(() => {
    if (open) {
      window.setTimeout(() => inputRef.current?.focus(), 20);
    } else {
      setQuery("");
    }
  }, [open]);

  const run = (command: PaletteCommand | undefined) => {
    if (command === undefined) {
      return;
    }
    onOpenChange(false);
    command.perform();
  };

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-black/60" />
        <DialogPrimitive.Content
          className="fixed left-1/2 top-24 z-50 w-[min(620px,92vw)] -translate-x-1/2 overflow-hidden rounded-[var(--radius-large)] border border-[var(--border-subtle)] bg-[var(--surface-overlay)] shadow-[var(--shadow-raised)]"
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setActiveIndex((index) => Math.min(index + 1, filtered.length - 1));
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              setActiveIndex((index) => Math.max(index - 1, 0));
            }
            if (event.key === "Enter") {
              event.preventDefault();
              run(filtered[activeIndex]);
            }
          }}
        >
          <DialogPrimitive.Title className="sr-only">Command palette</DialogPrimitive.Title>
          <DialogPrimitive.Description className="sr-only">
            Search the available actions and press Enter to run one.
          </DialogPrimitive.Description>
          <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-4 py-3">
            <Search aria-hidden className="size-4 text-[var(--text-muted)]" />
            <input
              ref={inputRef}
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
              }}
              placeholder="Search actions"
              aria-label="Search actions"
              aria-controls="command-palette-results"
              className="w-full bg-transparent text-sm text-[var(--text-primary)] outline-none placeholder:text-[var(--text-muted)]"
            />
          </div>
          <ul id="command-palette-results" className="max-h-80 overflow-auto p-1">
            {filtered.length === 0 && (
              <li className="px-3 py-6 text-center text-sm text-[var(--text-muted)]">
                No action matches that search.
              </li>
            )}
            {filtered.map((command, index) => (
              <li key={command.id}>
                <button
                  type="button"
                  onClick={() => {
                    run(command);
                  }}
                  onMouseEnter={() => {
                    setActiveIndex(index);
                  }}
                  aria-current={index === activeIndex}
                  className={`flex w-full items-center justify-between gap-3 rounded-[var(--radius-small)] px-3 py-2 text-left text-sm ${
                    index === activeIndex
                      ? "bg-[var(--surface-sunken)] text-[var(--text-primary)]"
                      : "text-[var(--text-secondary)]"
                  }`}
                >
                  <span>{command.label}</span>
                  <span className="text-xs text-[var(--text-muted)]">
                    {command.hint ?? command.group}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
