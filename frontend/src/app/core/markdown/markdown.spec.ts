import { parseInline, parseMarkdown } from './markdown';

describe('parseMarkdown', () => {
  it('parses the handbook subset', () => {
    const blocks = parseMarkdown(
      [
        '# Plan: Shop',
        '',
        '> **DRAFT** — not approved.',
        '',
        '| A | B |',
        '|---|---|',
        '| 1 | x \\| y |',
        '',
        '1. [ ] **Restore** (DBA, 1 h)',
        '   - Verify: ping',
        'Some text',
        'continued',
      ].join('\n'),
    );
    expect(blocks.map((b) => b.kind)).toEqual(['heading', 'quote', 'table', 'list', 'paragraph']);
    expect(blocks[2]).toEqual({ kind: 'table', header: ['A', 'B'], rows: [['1', 'x | y']] });
    expect(blocks[3]).toEqual({
      kind: 'list',
      items: [
        { text: '**Restore** (DBA, 1 h)', checkbox: true, indent: false },
        { text: 'Verify: ping', checkbox: false, indent: true },
      ],
    });
    expect(blocks[4]).toEqual({ kind: 'paragraph', text: 'Some text continued' });
  });

  it('parses bold and italic spans without HTML', () => {
    expect(parseInline('a **b** _c_ <i>d</i>')).toEqual([
      { text: 'a ' },
      { text: 'b', bold: true },
      { text: ' ' },
      { text: 'c', italic: true },
      { text: ' <i>d</i>' },
    ]);
  });
});
