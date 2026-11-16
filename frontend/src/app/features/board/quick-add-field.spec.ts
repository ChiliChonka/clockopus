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
