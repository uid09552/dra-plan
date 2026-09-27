import { TestBed } from '@angular/core/testing';

import { Theme } from './theme';

describe('Theme', () => {
  it('pins light or dark via a class on <html> and follows the system otherwise', () => {
    const theme = TestBed.inject(Theme);
    const root = document.documentElement.classList;

    theme.use('dark');
    TestBed.tick();
    expect(root.contains('theme-dark')).toBe(true);

    theme.use('light');
    TestBed.tick();
    expect(root.contains('theme-light')).toBe(true);
    expect(root.contains('theme-dark')).toBe(false);

    theme.use('system');
    TestBed.tick();
    expect(root.contains('theme-light') || root.contains('theme-dark')).toBe(false);
  });
});
