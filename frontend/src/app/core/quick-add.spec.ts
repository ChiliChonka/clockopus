import { appendTag, completeTag, parseQuickAdd, suggestTags, tagTokenAt } from './quick-add';

describe('parseQuickAdd', () => {
  it('splits title and tags', () => {
    expect(parseQuickAdd('Review payment PR #review #team-a')).toEqual({
      title: 'Review payment PR',
      tags: ['review', 'team-a'],
    });
  });

  it('takes tags anywhere, in lower case, and collapses whitespace', () => {
    expect(parseQuickAdd('  #support  Fix   login #Kunde-X bug #SUPPORT')).toEqual({
      title: 'Fix login bug',
      tags: ['support', 'kunde-x'],
    });
  });

  it('ignores a lone # and duplicate tags', () => {
    expect(parseQuickAdd('Call # Anna #a #A')).toEqual({ title: 'Call Anna', tags: ['a'] });
  });

  it('has an empty title when there are only tags', () => {
    expect(parseQuickAdd('#review')).toEqual({ title: '', tags: ['review'] });
  });
});

describe('tag completion', () => {
  it('finds the tag word under the caret', () => {
    const text = 'Fix bug #sup more';
    expect(tagTokenAt(text, 12)).toEqual({ prefix: 'sup', start: 8, end: 12 });
    expect(tagTokenAt(text, 10)).toEqual({ prefix: 's', start: 8, end: 12 });
    expect(tagTokenAt(text, 3)).toBeNull();
    expect(tagTokenAt('#', 1)).toEqual({ prefix: '', start: 0, end: 1 });
  });

  it('suggests known tags by prefix, without used ones', () => {
    const known = ['support', 'Supply', 'review', 'sup'];
    expect(suggestTags(known, 'SU', ['sup'])).toEqual(['support', 'Supply']);
    expect(suggestTags(known, '', [], 2)).toEqual(['support', 'Supply']);
  });

  it('replaces the token and puts the caret after it', () => {
    const text = 'Fix bug #sup more';
    const token = tagTokenAt(text, 12)!;
    expect(completeTag(text, token, 'support')).toEqual({
      text: 'Fix bug #support more',
      caret: 17,
    });
  });

  it('appends a quick-access tag once', () => {
    expect(appendTag('', 'review')).toBe('#review ');
    expect(appendTag('Fix bug', 'review')).toBe('Fix bug #review ');
    expect(appendTag('Fix bug #Review ', 'review')).toBe('Fix bug #Review ');
  });
});
