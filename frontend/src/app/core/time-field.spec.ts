import { ComponentFixture, TestBed } from '@angular/core/testing';
import { TimeField } from './time-field';

describe('TimeField', () => {
  let fixture: ComponentFixture<TimeField>;
  let input: HTMLInputElement;

  beforeEach(async () => {
    fixture = TestBed.createComponent(TimeField);
    fixture.componentRef.setInput('label', 'Start time');
    await fixture.whenStable();
    input = (fixture.nativeElement as HTMLElement).querySelector('input')!;
  });

  async function type(text: string) {
    input.value = text;
    input.dispatchEvent(new Event('input'));
    await fixture.whenStable();
  }

  it('should create a text field for HH:MM, not a browser time field', () => {
    expect(fixture.componentInstance).toBeTruthy();
    expect(input.type).toBe('text');
    expect(input.placeholder).toBe('HH:MM');
    expect(input.getAttribute('aria-label')).toBe('Start time');
  });

  it('writes a typed time as HH:MM when leaving the field', async () => {
    await type('930');
    expect(fixture.componentInstance.value()).toBe('930');
    input.dispatchEvent(new Event('blur'));
    await fixture.whenStable();
    expect(input.value).toBe('09:30');
    expect(fixture.componentInstance.value()).toBe('09:30');
  });

  it('marks what is not a time of day and leaves it for correction', async () => {
    await type('9 pm');
    input.dispatchEvent(new Event('blur'));
    await fixture.whenStable();
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(input.value).toBe('9 pm');
    await type('21:00');
    expect(input.getAttribute('aria-invalid')).toBeNull();
  });
});
