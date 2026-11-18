import { ComponentFixture, TestBed } from '@angular/core/testing';
import { entryView } from '../../../testing/fixtures';
import { EntryView } from '../../core/wire';
import { EntryChange, EntryRow } from './entry-row';

describe('EntryRow', () => {
  let fixture: ComponentFixture<EntryRow>;
  let saved: EntryChange[];
  const local = (h: number, m = 0) => new Date(2026, 10, 17, h, m);

  async function render(over: Partial<EntryView> = {}) {
    fixture = TestBed.createComponent(EntryRow);
    fixture.componentRef.setInput(
      'entry',
      entryView({ start: local(9).toISOString(), end: local(10).toISOString(), ...over }),
    );
    saved = [];
    fixture.componentInstance.save.subscribe((c) => saved.push(c));
    await fixture.whenStable();
    return fixture.nativeElement as HTMLElement;
  }

  function set(el: HTMLElement, selector: string, value: string) {
    const input = el.querySelector<HTMLInputElement>(selector)!;
    input.value = value;
    input.dispatchEvent(new Event('input'));
  }

  const saveButton = (el: HTMLElement) =>
    [...el.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === 'Save')!;

  /** The 24-hour text fields: start time, end time. */
  const times = (el: HTMLElement) => [
    ...el.querySelectorAll<HTMLInputElement>('app-time-field input'),
  ];

  it('should create with the values of the entry in 24-hour form and nothing to save', async () => {
    const el = await render({ end: local(17, 45).toISOString() });
    expect(fixture.componentInstance).toBeTruthy();
    const dates = el.querySelectorAll<HTMLInputElement>('input[type=date]');
    expect([...dates].map((d) => d.value)).toEqual(['2026-11-17', '2026-11-17']);
    expect(times(el).map((t) => t.value)).toEqual(['09:00', '17:45']);
    expect(el.querySelectorAll('input[type=datetime-local], input[type=time]').length).toBe(0);
    expect(el.querySelector('.duration')!.textContent).toBe('8h45m');
    expect(saveButton(el).disabled).toBe(true);
  });

  it('sends only what changed, typed times accepted', async () => {
    const el = await render();
    const [, end] = times(el);
    end.value = '1030';
    end.dispatchEvent(new Event('input'));
    set(el, 'input[type=text]:not([inputmode])', 'pairing');
    await fixture.whenStable();
    expect(saveButton(el).disabled).toBe(false);
    saveButton(el).click();
    expect(saved).toEqual([{ end: local(10, 30), note: 'pairing' }]);
  });

  it('refuses an end before the start and an invalid time', async () => {
    const el = await render();
    const [, end] = times(el);
    end.value = '08:00';
    end.dispatchEvent(new Event('input'));
    await fixture.whenStable();
    saveButton(el).click();
    await fixture.whenStable();
    expect(saved).toEqual([]);
    expect(el.querySelector('.error')!.textContent).toContain('after the start');

    end.value = '25:00';
    end.dispatchEvent(new Event('input'));
    await fixture.whenStable();
    saveButton(el).click();
    await fixture.whenStable();
    expect(saved).toEqual([]);
    expect(el.querySelector('.error')!.textContent).toContain('HH:MM');
    expect(end.getAttribute('aria-invalid')).toBe('true');
  });

  it('shows a running entry without an end field', async () => {
    const el = await render({ end: null });
    expect(times(el).length).toBe(1);
    expect(el.querySelectorAll('input[type=date]').length).toBe(1);
    expect(el.querySelector('.running')!.textContent).toBe('running');
  });

  it('shows the history on demand', async () => {
    const el = await render({
      history: [
        {
          changed_at: local(18).toISOString(),
          reason: 'forgotten-timer',
          start: local(9).toISOString(),
          end: null,
        },
      ],
    });
    expect(el.querySelector('.history')).toBeNull();
    const toggle = [...el.querySelectorAll('button')].find((b) =>
      b.textContent!.includes('change'),
    )!;
    expect(toggle.textContent!.trim()).toBe('1 change');
    toggle.click();
    await fixture.whenStable();
    expect(el.querySelector('.history li')!.textContent!.replace(/\s+/g, ' ').trim()).toBe(
      '2026-11-17 18:00: was 2026-11-17 09:00 – running (forgotten timer)',
    );
  });
});
