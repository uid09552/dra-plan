import { TestBed } from '@angular/core/testing';

import { I18n, detectLanguage } from './i18n';

describe('detectLanguage', () => {
  it('prefers a stored choice', () => {
    expect(detectLanguage('de', ['en-US'])).toBe('de');
    expect(detectLanguage('en', ['de-DE'])).toBe('en');
  });

  it('follows the browser language', () => {
    expect(detectLanguage(null, ['de-AT', 'en'])).toBe('de');
    expect(detectLanguage(null, ['fr-FR', 'de'])).toBe('de');
    expect(detectLanguage(null, ['en-GB'])).toBe('en');
  });

  it('falls back to English', () => {
    expect(detectLanguage(null, ['fr-FR'])).toBe('en');
    expect(detectLanguage('xx', [])).toBe('en');
  });
});

describe('I18n', () => {
  it('translates, interpolates and switches language', () => {
    const i18n = TestBed.inject(I18n);
    i18n.use('en');
    expect(i18n.t('nav.services')).toBe('IT services');
    expect(i18n.t('dashboard.workflowAt', { percent: 40 })).toBe('Workflow 40% complete');
    i18n.use('de');
    expect(i18n.t('nav.services')).toBe('IT-Services');
    TestBed.tick();
    expect(document.documentElement.lang).toBe('de');
    expect(i18n.t('does.not.exist')).toBe('does.not.exist');
  });
});
