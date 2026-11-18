import { ComponentFixture, TestBed } from '@angular/core/testing';
import { tagView } from '../../../testing/fixtures';
import { QuickAddField, QuickAddSubmit } from './quick-add-field';

describe('QuickAddField', () => {
  let fixture: ComponentFixture<QuickAddField>;
  let el: HTMLElement;
  let input: HTMLInputElement;
  let submitted: QuickAddSubmit[];

  beforeEach(async () => {
    fixture = TestBed.createComponent(QuickAddField);
    fixture.componentRef.setInput('knownTags', [
      tagView('support'),
      tagView('Supply'),
      tagView('review'),
    ]);
    fixture.componentRef.setInput('quickTags', [tagView('review'), tagView('support')]);
    submitted = [];
    fixture.componentInstance.submitted.subscribe((s) => submitted.push(s));
    await fixture.whenStable();
    el = fixture.nativeElement as HTMLElement;
    input = el.querySelector('input')!;
  });

  async function type(value: string) {
    input.focus();
    input.value = value;
    input.setSelectionRange(value.length, value.length);
    input.dispatchEvent(new Event('input'));
    await fixture.whenStable();
  }

  async function key(k: string, init: KeyboardEventInit = {}) {
    const event = new KeyboardEvent('keydown', { key: k, cancelable: true, ...init });
    input.dispatchEvent(event);
    await fixture.whenStable();
    return event;
  }

  function suggestions(): string[] {
    return [...el.querySelectorAll('.suggestions button')].map((b) => b.textContent!.trim());
  }

  it('should create', () => {
    expect(fixture.componentInstance).toBeTruthy();
  });

  it('suggests known tags while typing a #word and completes with Tab', async () => {
    await type('Fix bug #su');
    expect(suggestions()).toEqual(['#support', '#Supply']);

    const tab = await key('Tab');
    expect(tab.defaultPrevented).toBe(true);
    expect(input.value).toBe('Fix bug #support ');
    expect(fixture.componentInstance.text()).toBe('Fix bug #support ');
    expect(suggestions()).toEqual([]);
  });

  it('does not suggest tags that are already in the text', async () => {
    await type('#support Fix #su');
    expect(suggestions()).toEqual(['#Supply']);
  });

  it('leaves Tab alone without suggestions', async () => {
    await type('Fix bug');
    expect((await key('Tab')).defaultPrevented).toBe(false);
  });

  const active = () =>
    [...el.querySelectorAll('.suggestions [aria-selected=true] button')].map((b) =>
      b.textContent!.trim(),
    );

  it('arrow keys choose a suggestion; Enter takes it instead of creating the task', async () => {
    await type('Fix bug #su');
    expect(active()).toEqual([]);
    const down = await key('ArrowDown');
    expect(down.defaultPrevented).toBe(true);
    expect(active()).toEqual(['#support']);
    expect(input.getAttribute('aria-activedescendant')).toBe('quick-add-option-0');
    await key('ArrowDown');
    expect(active()).toEqual(['#Supply']);
    await key('ArrowDown');
    // Wraps around.
    expect(active()).toEqual(['#support']);
    await key('ArrowUp');
    expect(active()).toEqual(['#Supply']);

    const enter = await key('Enter');
    expect(enter.defaultPrevented).toBe(true);
    expect(input.value).toBe('Fix bug #Supply ');
    // Enter took the tag; nothing was created.
    expect(submitted).toEqual([]);

    // With no suggestion open, Enter creates the task again.
    await key('Enter');
    expect(submitted).toEqual([{ text: 'Fix bug #Supply ', start: true }]);
  });

  it('keeps the choice when the tags are reloaded unchanged in the background', async () => {
    // Regression (seen in the browser): every board reload built a new, equal list and reset
    // the choice, so ↓ seemed to do nothing right after the page loaded.
    await type('#su');
    await key('ArrowDown');
    expect(active()).toEqual(['#support']);
    fixture.componentRef.setInput('knownTags', [
      tagView('support'),
      tagView('Supply'),
      tagView('review'),
    ]);
    await fixture.whenStable();
    expect(active()).toEqual(['#support']);
    // A different list does reset it.
    fixture.componentRef.setInput('knownTags', [tagView('support'), tagView('sun')]);
    await fixture.whenStable();
    expect(active()).toEqual([]);
  });

  it('Tab takes the chosen suggestion, ArrowUp starts at the last one', async () => {
    await type('#su');
    await key('ArrowUp');
    expect(active()).toEqual(['#Supply']);
    await key('Tab');
    expect(input.value).toBe('#Supply ');
  });

  it('Escape closes the list; typing opens it again', async () => {
    await type('#re');
    expect(suggestions()).toEqual(['#review']);
    await key('Escape');
    expect(suggestions()).toEqual([]);
    await type('#rev');
    expect(suggestions()).toEqual(['#review']);
  });

  it('Enter creates and starts, Alt+Enter only creates', async () => {
    await type('Review PR #review');
    await key('Enter');
    await key('Enter', { altKey: true });
    expect(submitted).toEqual([
      { text: 'Review PR #review', start: true },
      { text: 'Review PR #review', start: false },
    ]);
  });

  it('sends nothing for an empty field', async () => {
    await type('   ');
    await key('Enter');
    expect(submitted).toEqual([]);
  });

  it('inserts a frequent tag from the bar once', async () => {
    await type('Fix bug');
    const chip = [...el.querySelectorAll<HTMLButtonElement>('.quick-tags button')].find(
      (b) => b.textContent === '#review',
    )!;
    chip.click();
    chip.click();
    await fixture.whenStable();
    expect(fixture.componentInstance.text()).toBe('Fix bug #review ');
  });
});
