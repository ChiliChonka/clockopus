// Quick-add syntax (docs/SPEC.md, section 4.1): words starting with `#` become tags, the rest is
// the title. `Review payment PR #review #team-a` → title `Review payment PR`, tags review, team-a.

export interface QuickAdd {
  title: string;
  tags: string[];
}

export function parseQuickAdd(text: string): QuickAdd {
  const words = text.split(/\s+/).filter((w) => w.length > 0);
  const tags: string[] = [];
  const title: string[] = [];
  for (const word of words) {
    if (word.startsWith('#') && word.length > 1) {
      const tag = word.slice(1);
      if (!tags.some((t) => t.toLowerCase() === tag.toLowerCase())) tags.push(tag);
    } else if (word !== '#') {
      title.push(word);
    }
  }
  return { title: title.join(' '), tags };
}

/** The `#tag` word the caret is in, for auto-completion; `null` outside a tag. */
export interface TagToken {
  prefix: string;
  start: number;
  end: number;
}

export function tagTokenAt(text: string, caret: number): TagToken | null {
  let start = caret;
  while (start > 0 && !/\s/.test(text[start - 1])) start--;
  let end = caret;
  while (end < text.length && !/\s/.test(text[end])) end++;
  const word = text.slice(start, end);
  if (!word.startsWith('#')) return null;
  return { prefix: word.slice(1, caret - start), start, end };
}

/** Tag names starting with `prefix` (case-insensitive), without those already in the text. */
export function suggestTags(known: string[], prefix: string, used: string[], limit = 6): string[] {
  const p = prefix.toLowerCase();
  const usedLower = used.map((u) => u.toLowerCase());
  return known
    .filter((name) => name.toLowerCase().startsWith(p) && !usedLower.includes(name.toLowerCase()))
    .slice(0, limit);
}

/** Replaces the token with `#tag ` and returns the new text and caret position. */
export function completeTag(
  text: string,
  token: TagToken,
  tag: string,
): { text: string; caret: number } {
  const before = text.slice(0, token.start);
  const after = text.slice(token.end).replace(/^\s*/, '');
  const inserted = `#${tag} `;
  return { text: before + inserted + after, caret: before.length + inserted.length };
}

/** Appends `#tag` to the text (quick-access bar), unless it is already there. */
export function appendTag(text: string, tag: string): string {
  if (parseQuickAdd(text).tags.some((t) => t.toLowerCase() === tag.toLowerCase())) return text;
  const base = text.trimEnd();
  return `${base}${base ? ' ' : ''}#${tag} `;
}
