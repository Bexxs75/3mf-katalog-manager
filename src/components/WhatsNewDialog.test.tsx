import { uiSettings } from '../lib/uiSettings';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { WhatsNewDialog } from './WhatsNewDialog';
import { useWhatsNew } from '../hooks/useWhatsNew';
import { currentVersion, releaseNotes, type ReleaseGroups } from '../lib/releaseNotes';

// A pre-release (0.18.6-1) uses the notes of its base version.
const noteKey = releaseNoteKey(currentVersion);
// A fix-only release has no "new" entries, so use the first entry of any group.
const firstNote = (g: ReleaseGroups) => g.new[0] ?? g.improved[0] ?? g.fixed[0];
import { LAST_SEEN_VERSION_KEY as key, releaseNoteKey } from '../lib/releaseNotesSelection';
import { openReleaseUrl } from '../lib/api/update';
vi.mock('../lib/api/update', () => ({ openReleaseUrl: vi.fn().mockResolvedValue(undefined) }));

function Harness({ wait = false, firstRun = false, web = false, reopenBlocked = false }) {
  const state = useWhatsNew({ wait, firstRun, web, reopenBlocked });
  return <><button onClick={state.reopen}>Reopen</button><output>{state.settled ? 'settled' : 'waiting'}</output>
    {state.versions.length > 0 && <WhatsNewDialog versions={state.versions} onClose={state.close} />}</>;
}
const mount = (props = {}) => render(<LanguageProvider><Harness {...props} /></LanguageProvider>);
beforeEach(() => { localStorage.clear(); vi.unstubAllEnvs(); });
it.each([['de', de], ['en', en]] as const)('renders %s, focuses Got it, marks only on close and reopens current', async (language, t) => {
  uiSettings.setItem('3mf-katalog-language', language);
  mount();
  expect(screen.getByRole('dialog')).toHaveAccessibleName(t.whatsNewTitle.replace('{version}', currentVersion));
  expect(screen.getByText(firstNote(releaseNotes[noteKey].notes[language]!))).toBeVisible();
  expect(uiSettings.getItem(key)).toBeNull();
  const button = screen.getByRole('button', { name: t.whatsNewClose });
  await waitFor(() => expect(button).toHaveFocus());
  fireEvent.click(button);
  expect(uiSettings.getItem(key)).toBe(currentVersion);
  expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.click(screen.getByText('Reopen'));
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(screen.queryByRole('dialog')).toBeNull();
});
it('waits for preceding dialogs and suppresses fresh desktop installs', () => {
  const view = mount({ wait: true, firstRun: true });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(uiSettings.getItem(key)).toBeNull();
  view.rerender(<LanguageProvider><Harness firstRun /></LanguageProvider>);
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(uiSettings.getItem(key)).toBe(currentVersion);
  expect(screen.getByText('settled')).toBeVisible();
});
it('web first start marks, restart stays quiet, older marker shows automatically', () => {
  let view = mount({ web: true });
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(uiSettings.getItem(key)).toBe(currentVersion);
  view.unmount(); view = mount({ web: true });
  expect(screen.queryByRole('dialog')).toBeNull();
  view.unmount(); uiSettings.setItem(key, '0.18.0'); mount({ web: true });
  expect(screen.getByRole('dialog')).toBeVisible();
});
it('manual reopen after downgrade preserves the newer marker', () => {
  const newerVersion = `${Number(currentVersion.split('.')[0]) + 1}.0.0`;
  uiSettings.setItem(key, newerVersion); mount();
  expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.click(screen.getByText('Reopen'));
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(uiSettings.getItem(key)).toBe(newerVersion);
  expect(screen.queryByRole('dialog')).toBeNull();
});
it('uses the desktop release command and a normal web link', async () => {
  const view = mount();
  const link = screen.getByRole('link');
  await act(async () => { fireEvent.click(link); });
  expect(openReleaseUrl).toHaveBeenCalledWith(`https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v${noteKey}`);
  view.unmount(); vi.stubEnv('VITE_TARGET', 'web');
  mount();
  expect(screen.getByRole('link')).toHaveAttribute('href', `https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v${noteKey}`);
});
it('falls back to English and hides empty groups', () => {
  uiSettings.setItem('3mf-katalog-language', 'fr');
  const original = releaseNotes[noteKey].notes.fr;
  delete releaseNotes[noteKey].notes.fr;
  try {
    mount();
    expect(screen.getByText(firstNote(releaseNotes[noteKey].notes.en))).toBeVisible();
    expect(screen.getAllByRole('list')).toHaveLength(Object.values(releaseNotes[noteKey].notes.en).filter(notes => notes.length > 0).length);
  } finally { releaseNotes[noteKey].notes.fr = original; }
});
it('the X closes and remembers the version', () => {
  uiSettings.setItem('3mf-katalog-language', 'de'); mount();
  fireEvent.click(screen.getByRole('button', { name: de.impClose }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(uiSettings.getItem(key)).toBe(currentVersion);
});
it('renders separate dated sections for skipped releases', () => {
  uiSettings.setItem('3mf-katalog-language', 'en');
  render(<LanguageProvider><WhatsNewDialog versions={['0.18.2', '0.18.1']} onClose={vi.fn()} /></LanguageProvider>);
  expect(screen.getByRole('dialog')).toHaveAccessibleName(en.whatsNew);
  expect(screen.getAllByRole('heading', { level: 3 }).map(h => h.textContent)).toEqual([
    '0.18.2 · November 17, 2026', '0.18.1 · November 16, 2026',
  ]);
  expect(screen.getAllByRole('link').map(a => a.getAttribute('href'))).toEqual([
    'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.18.2',
    'https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.18.1',
  ]);
});

it('ignores reopen until the startup decision and other startup dialogs are settled', () => {
  const view = mount({ wait: true, firstRun: true });
  fireEvent.click(screen.getByText('Reopen'));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(uiSettings.getItem(key)).toBeNull();
  view.rerender(<LanguageProvider><Harness firstRun reopenBlocked /></LanguageProvider>);
  fireEvent.click(screen.getByText('Reopen'));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(uiSettings.getItem(key)).toBe(currentVersion);
  view.rerender(<LanguageProvider><Harness firstRun /></LanguageProvider>);
  fireEvent.click(screen.getByText('Reopen'));
  expect(screen.getByRole('dialog')).toBeVisible();
});
it('renders repeated note text without duplicate React keys', () => {
  uiSettings.setItem('3mf-katalog-language', 'en');
  const original = releaseNotes[noteKey].notes.en.new;
  const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
  releaseNotes[noteKey].notes.en.new = ['Repeated note', 'Repeated note'];
  try {
    mount();
    expect(screen.getAllByText('Repeated note')).toHaveLength(2);
    expect(errors).not.toHaveBeenCalled();
  } finally {
    releaseNotes[noteKey].notes.en.new = original;
    errors.mockRestore();
  }
});
