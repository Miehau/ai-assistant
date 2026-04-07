import { marked } from "marked";

export interface MarkdownBlock {
  id: string;
  content: string;
  start: number;
  end: number;
}

function toBlock(content: string, start: number, end: number): MarkdownBlock {
  return {
    id: `${start}:${end}`,
    content,
    start,
    end,
  };
}

export function parseMarkdownBlocks(text: string): MarkdownBlock[] {
  if (!text) return [];

  try {
    const tokens = marked.lexer(text, {
      gfm: true,
      breaks: true,
    });

    if (tokens.length === 0) {
      return [toBlock(text, 0, text.length)];
    }

    const blocks: MarkdownBlock[] = [];
    let cursor = 0;

    for (const token of tokens) {
      const raw = typeof token.raw === "string" ? token.raw : "";
      if (!raw) continue;

      const start = cursor;
      const end = cursor + raw.length;
      blocks.push(toBlock(raw, start, end));
      cursor = end;
    }

    if (cursor < text.length) {
      blocks.push(toBlock(text.slice(cursor), cursor, text.length));
    }

    return blocks.length > 0 ? blocks : [toBlock(text, 0, text.length)];
  } catch (error) {
    console.error("Failed to parse markdown blocks:", error);
    return [toBlock(text, 0, text.length)];
  }
}
