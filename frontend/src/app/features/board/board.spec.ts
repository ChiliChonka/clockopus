import { TestBed } from '@angular/core/testing';
import { Board } from './board';

describe('Board', () => {
  it('should create and show the empty state', async () => {
    const fixture = TestBed.createComponent(Board);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(fixture.componentInstance).toBeTruthy();
    expect(el.querySelector('h2')?.textContent).toBe('No tasks yet');
  });
});
