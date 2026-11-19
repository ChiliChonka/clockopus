import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { App } from './app';
import { AuthService } from './core/auth.service';
import { Me, PublicConfig, SessionService } from './core/session.service';

describe('App', () => {
  const me = signal<Me | null>(null);
  const config = signal<PublicConfig | null>(null);
  const error = signal<string | null>(null);
  const load = vi.fn().mockResolvedValue(undefined);
  const auth = { enabled: signal(false), logout: vi.fn() };

  beforeEach(async () => {
    me.set(null);
    config.set(null);
    error.set(null);
    load.mockClear();
    auth.enabled.set(false);
    auth.logout.mockClear();
    await TestBed.configureTestingModule({
      imports: [App],
      providers: [
        provideRouter([]),
        { provide: SessionService, useValue: { me, config, error, load } },
        { provide: AuthService, useValue: auth },
      ],
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

  it('links to all views', async () => {
    const fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    const links = [...(fixture.nativeElement as HTMLElement).querySelectorAll('nav a')];
    expect(links.map((a) => [a.textContent!.trim(), a.getAttribute('href')])).toEqual([
      ['Board', '/'],
      ['Day', '/day'],
      ['Week', '/week'],
      ['Month', '/month'],
      ['Notes', '/notes'],
      ['⚙', '/settings'],
    ]);
    expect(links[5].getAttribute('aria-label')).toBe('Settings');
  });

  it('offers sign-out only when sign-in is enabled', async () => {
    let fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    expect((fixture.nativeElement as HTMLElement).querySelector('.logout')).toBeNull();

    auth.enabled.set(true);
    fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    const button = (fixture.nativeElement as HTMLElement).querySelector<HTMLButtonElement>(
      '.logout',
    )!;
    expect(button.textContent!.trim()).toBe('Sign out');
    button.click();
    expect(auth.logout).toHaveBeenCalledTimes(1);
  });

  it('shows the running version from the server, linked to the releases', async () => {
    let fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    expect((fixture.nativeElement as HTMLElement).querySelector('footer')).toBeNull();

    // Literal as the backend sends it (backend/src/app.rs).
    config.set(JSON.parse('{"version":"0.2.0","auth":"none"}'));
    fixture = TestBed.createComponent(App);
    await fixture.whenStable();
    const link = (fixture.nativeElement as HTMLElement).querySelector('footer a')!;
    expect(link.textContent!.trim()).toBe('clockopus 0.2.0');
    expect(link.getAttribute('href')).toBe('https://github.com/ChiliChonka/clockopus/releases');
  });
});
