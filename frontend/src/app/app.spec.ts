import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { App } from './app';
import { Me, SessionService } from './core/session.service';

describe('App', () => {
  const me = signal<Me | null>(null);
  const error = signal<string | null>(null);
  const load = vi.fn().mockResolvedValue(undefined);

  beforeEach(async () => {
    me.set(null);
    error.set(null);
    load.mockClear();
    await TestBed.configureTestingModule({
      imports: [App],
      providers: [provideRouter([]), { provide: SessionService, useValue: { me, error, load } }],
    }).compileComponents();
  });

  it('should create and load the session', () => {
    const fixture = TestBed.createComponent(App);
    expect(fixture.componentInstance).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it('shows the brand and the signed-in user', async () => {
    me.set({ id: 'local', name: 'Local user' });
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.brand')?.textContent).toContain('clockopus');
    expect(el.querySelector('.user')?.textContent).toBe('Local user');
    expect(el.querySelector('[role=alert]')).toBeNull();
  });

  it('shows an error when the server is not reachable', async () => {
    error.set('The clockopus server is not reachable. Please try again in a moment.');
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    const alert = (fixture.nativeElement as HTMLElement).querySelector('[role=alert]');
    expect(alert?.textContent).toContain('not reachable');
  });
});
