/**
 * Minimal Markdown block parser for the handbook preview (the subset the backend renderer emits:
 * headings, paragraphs, quotes, lists, checkboxes and tables). Blocks are rendered with Angular
 * templates, never via innerHTML, so plan content cannot inject markup.
 */
export type Block =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'paragraph'; text: string }
  | { kind: 'quote'; text: string }
  | { kind: 'list'; items: { text: string; checkbox: boolean; indent: boolean }[] }
  | { kind: 'table'; header: string[]; rows: string[][] };

/** Inline span: `**bold**`, `_italic_` or plain text. */
export interface Span {
  text: string;
  bold?: boolean;
  italic?: boolean;
}

export function parseMarkdown(markdown: string): Block[] {
  const blocks: Block[] = [];
  const lines = markdown.split(/\r?\n/);
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (!line.trim()) {
      i++;
    } else if (heading) {
      blocks.push({ kind: 'heading', level: heading[1].length, text: heading[2] });
      i++;
    } else if (line.startsWith('|')) {
      const rows: string[][] = [];
      for (; i < lines.length && lines[i].startsWith('|'); i++) {
        if (!/^\|[\s|:-]+\|$/.test(lines[i])) {
          rows.push(splitRow(lines[i]));
        }
      }
      blocks.push({ kind: 'table', header: rows[0] ?? [], rows: rows.slice(1) });
    } else if (line.startsWith('>')) {
      blocks.push({ kind: 'quote', text: line.replace(/^>\s?/, '') });
      i++;
    } else if (isListItem(line)) {
      const items: { text: string; checkbox: boolean; indent: boolean }[] = [];
      for (; i < lines.length && isListItem(lines[i]); i++) {
        const indent = /^\s{2,}/.test(lines[i]);
        const text = lines[i].trim().replace(/^(-|\d+\.)\s+/, '');
        const checkbox = text.startsWith('[ ] ');
        items.push({ text: checkbox ? text.slice(4) : text, checkbox, indent });
      }
      blocks.push({ kind: 'list', items });
    } else {
      const text: string[] = [];
      for (; i < lines.length && isParagraphLine(lines[i]); i++) {
        text.push(lines[i].trim());
      }
      blocks.push({ kind: 'paragraph', text: text.join(' ') });
    }
  }
  return blocks;
}

export function parseInline(text: string): Span[] {
  const spans: Span[] = [];
  const pattern = /\*\*(.+?)\*\*|(?<![\w])_(.+?)_(?![\w])/g;
  let last = 0;
  for (const m of text.matchAll(pattern)) {
    if (m.index > last) {
      spans.push({ text: text.slice(last, m.index) });
    }
    spans.push(m[1] !== undefined ? { text: m[1], bold: true } : { text: m[2], italic: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) {
    spans.push({ text: text.slice(last) });
  }
  return spans;
}

function splitRow(line: string): string[] {
  return line
    .replace(/^\||\|$/g, '')
    .split(/(?<!\\)\|/)
    .map((c) => c.trim().replace(/\\\|/g, '|'));
}

function isListItem(line: string): boolean {
  return /^\s*(-|\d+\.)\s+/.test(line);
}

function isParagraphLine(line: string): boolean {
  return !!line.trim() && !/^(#{1,6}\s|\||>)/.test(line) && !isListItem(line);
}
