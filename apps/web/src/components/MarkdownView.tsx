import { Fragment } from "react";
import type { ReactNode } from "react";

interface Block {
  kind: "heading" | "paragraph" | "list" | "code" | "quote";
  level?: number;
  language?: string;
  lines: string[];
}

function parseBlocks(markdown: string): Block[] {
  const blocks: Block[] = [];
  const lines = markdown.split(/\r?\n/);
  let index = 0;
  while (index < lines.length) {
    const line = lines[index] ?? "";
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      index += 1;
      continue;
    }
    if (trimmed.startsWith("```")) {
      const language = trimmed.slice(3).trim();
      const body: string[] = [];
      index += 1;
      while (index < lines.length && !(lines[index] ?? "").trim().startsWith("```")) {
        body.push(lines[index] ?? "");
        index += 1;
      }
      index += 1;
      blocks.push({ kind: "code", language, lines: body });
      continue;
    }
    const headingMatch = /^(#{1,6})\s+(.*)$/.exec(trimmed);
    if (headingMatch) {
      blocks.push({
        kind: "heading",
        level: headingMatch[1]?.length ?? 1,
        lines: [headingMatch[2] ?? ""],
      });
      index += 1;
      continue;
    }
    if (/^[-*+]\s+/.test(trimmed) || /^\d+\.\s+/.test(trimmed)) {
      const items: string[] = [];
      while (
        index < lines.length &&
        (/^[-*+]\s+/.test((lines[index] ?? "").trim()) ||
          /^\d+\.\s+/.test((lines[index] ?? "").trim()))
      ) {
        items.push((lines[index] ?? "").trim().replace(/^([-*+]|\d+\.)\s+/, ""));
        index += 1;
      }
      blocks.push({ kind: "list", lines: items });
      continue;
    }
    if (trimmed.startsWith(">")) {
      const quote: string[] = [];
      while (index < lines.length && (lines[index] ?? "").trim().startsWith(">")) {
        quote.push((lines[index] ?? "").trim().replace(/^>\s?/, ""));
        index += 1;
      }
      blocks.push({ kind: "quote", lines: quote });
      continue;
    }
    const paragraph: string[] = [];
    while (index < lines.length && (lines[index] ?? "").trim().length > 0) {
      const next = (lines[index] ?? "").trim();
      if (
        next.startsWith("```") ||
        /^#{1,6}\s/.test(next) ||
        /^[-*+]\s+/.test(next) ||
        next.startsWith(">")
      ) {
        break;
      }
      paragraph.push(next);
      index += 1;
    }
    blocks.push({ kind: "paragraph", lines: paragraph });
  }
  return blocks;
}

function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|_[^_]+_)/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null = pattern.exec(text);
  let counter = 0;
  while (match !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    const key = `${keyPrefix}-${String(counter)}`;
    counter += 1;
    if (token.startsWith("`")) {
      nodes.push(
        <code
          key={key}
          className="rounded bg-[var(--surface-sunken)] px-1 py-0.5 text-[var(--accent-secondary)]"
        >
          {token.slice(1, -1)}
        </code>,
      );
    } else if (token.startsWith("**")) {
      nodes.push(
        <strong key={key} className="font-semibold text-[var(--text-primary)]">
          {token.slice(2, -2)}
        </strong>,
      );
    } else {
      nodes.push(
        <em key={key} className="italic">
          {token.slice(1, -1)}
        </em>,
      );
    }
    lastIndex = match.index + token.length;
    match = pattern.exec(text);
  }
  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

export interface MarkdownViewProps {
  markdown: string;
  className?: string;
}

export function MarkdownView({ markdown, className }: MarkdownViewProps) {
  const blocks = parseBlocks(markdown);
  return (
    <div className={className}>
      {blocks.map((block, blockIndex) => {
        const key = `block-${String(blockIndex)}`;
        if (block.kind === "heading") {
          const level = block.level ?? 2;
          const text = block.lines[0] ?? "";
          const sizes = ["text-xl", "text-lg", "text-base", "text-sm", "text-sm", "text-sm"];
          const Heading = level === 1 ? "h2" : level === 2 ? "h3" : "h4";
          return (
            <Heading
              key={key}
              className={`mb-2 mt-5 font-semibold text-[var(--text-primary)] first:mt-0 ${sizes[level - 1] ?? "text-sm"}`}
            >
              {renderInline(text, key)}
            </Heading>
          );
        }
        if (block.kind === "code") {
          return (
            <pre
              key={key}
              className="scrollbar-slim my-3 overflow-x-auto rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-sunken)] p-3 text-[12px] leading-relaxed"
            >
              <code>{block.lines.join("\n")}</code>
            </pre>
          );
        }
        if (block.kind === "list") {
          return (
            <ul
              key={key}
              className="my-2 list-disc space-y-1 pl-5 text-sm text-[var(--text-secondary)]"
            >
              {block.lines.map((item, itemIndex) => (
                <li key={`${key}-${String(itemIndex)}`}>
                  {renderInline(item, `${key}-${String(itemIndex)}`)}
                </li>
              ))}
            </ul>
          );
        }
        if (block.kind === "quote") {
          return (
            <blockquote
              key={key}
              className="my-3 border-l-2 border-[var(--accent-primary)] pl-3 text-sm italic text-[var(--text-secondary)]"
            >
              {block.lines.map((line, lineIndex) => (
                <Fragment key={`${key}-${String(lineIndex)}`}>
                  {renderInline(line, `${key}-${String(lineIndex)}`)}
                  <br />
                </Fragment>
              ))}
            </blockquote>
          );
        }
        return (
          <p key={key} className="my-2 text-sm leading-relaxed text-[var(--text-secondary)]">
            {block.lines.map((line, lineIndex) => (
              <Fragment key={`${key}-${String(lineIndex)}`}>
                {renderInline(line, `${key}-${String(lineIndex)}`)}{" "}
              </Fragment>
            ))}
          </p>
        );
      })}
    </div>
  );
}
