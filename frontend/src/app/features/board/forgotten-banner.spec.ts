import { ComponentFixture, TestBed } from '@angular/core/testing';
import { forgotten } from '../../../testing/fixtures';
import { Forgotten } from '../../core/wire';
import { ForgottenBanner } from './forgotten-banner';

describe('ForgottenBanner', () => {
  let fixture: ComponentFixture<ForgottenBanner>;
  // Local times, so the labels hold in any time zone.
  const start = new Date(2026, 10, 16, 9, 12);
  const suggested = new Date(2026, 10, 16, 17, 0);
  const now = new Date(2026, 10, 17, 8, 52);
  let stops: Date[];
  let keeps: number;

  async function render(over: Partial<Forgotten> = {}) {
    fixture = TestBed.createComponent(ForgottenBanner);
    fixture.componentRef.setInput(
      'forgotten',
      forgotten({ start: start.toISOString(), suggested_end: suggested.toISOString(), ...over }),
    );
    fixture.componentRef.setInput('now', now.getTime());
    stops = [];
    keeps = 0;
    fixture.componentInstance.stopAt.subscribe((d) => stops.push(d));
    fixture.componentInstance.keep.subscribe(() => keeps++);
    await fixture.whenStable();
    return fixture.nativeElement as HTMLElement;
  }

  const button = (el: HTMLElement, text: string) =>
    [...el.querySelectorAll('button')].find((b) => b.textContent!.trim().startsWith(text))!;

  it('should create and describe the forgotten timer', async () => {
    const el = await render();
    expect(fixture.componentInstance).toBeTruthy();
    expect(el.querySelector('p')!.textContent!.replace(/\s+/g, ' ').trim()).toBe(
      'Your timer for Fix login bug has been running since yesterday 09:12 (23 h 40 min).',
    );
    expect(el.querySelector('[role=alert]')).not.toBeNull();
  });

  it('offers the workday end of the start day', async () => {
    const el = await render();
    const stop = button(el, 'Stop at');
    expect(stop.textContent!.trim()).toBe('Stop at yesterday 17:00');
    stop.click();
    expect(stops.map((d) => d.getTime())).toEqual([suggested.getTime()]);
  });

  it('has no suggestion when the workday end is not usable', async () => {
    const el = await render({ suggested_end: null });
    expect(button(el, 'Stop at')).toBeUndefined();
  });

  it('lets the user pick an end and checks it', async () => {
    const el = await render();
    button(el, 'Pick end time').click();
    await fixture.whenStable();
    const date = el.querySelector<HTMLInputElement>('input[type=date]')!;
    const input = el.querySelector<HTMLInputElement>('app-time-field input')!;
    expect(date.value).toBe('2026-11-16');
    expect(input.value).toBe('17:00');

    input.value = '0800';
    input.dispatchEvent(new Event('input'));
    // A detached form is not submitted by jsdom on click; send the event the browser would.
    el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await fixture.whenStable();
    expect(stops).toEqual([]);
    expect(el.textContent).toContain('Pick a time after the start');

    input.value = '18:30';
    input.dispatchEvent(new Event('input'));
    el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    expect(stops.map((d) => d.getTime())).toEqual([new Date(2026, 10, 16, 18, 30).getTime()]);
  });

  it('keeps it running on request', async () => {
    const el = await render();
    button(el, 'Keep running').click();
    expect(keeps).toBe(1);
  });
});
