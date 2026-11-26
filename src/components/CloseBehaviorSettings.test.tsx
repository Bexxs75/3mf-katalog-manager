import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { CloseBehaviorSettings } from './CloseBehaviorSettings';
import { SettingsToolsContext } from './SettingsTools';
import * as api from '../lib/api/closeBehavior';

vi.mock('../lib/api/closeBehavior');

it('keeps radio focus and ignores further choices until the save completes', async () => {
  const initial: api.CloseBehavior = { action: 'exit', platform: 'windows', supported: true, trayAvailable: true };
  vi.mocked(api.getCloseBehavior).mockResolvedValue(initial);
  let finish!: (value: api.CloseBehavior) => void;
  vi.mocked(api.setCloseBehavior).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  render(<LanguageProviderWithDiagnostics><CloseBehaviorSettings /></LanguageProviderWithDiagnostics>);
  const radios = await screen.findAllByRole('radio');
  for (const option of radios) expect(option).toHaveClass('print-technology-radio', 'mt-1', 'focus-visible:outline');
  const radio = radios[1];
  radio.focus();
  fireEvent.click(radio);
  const group = screen.getByRole('group');
  expect(group).toHaveAttribute('aria-busy', 'true');
  expect(group).toHaveClass('opacity-60');
  for (const option of radios) expect(option).toBeEnabled();
  expect(document.activeElement).toBe(radio);
  fireEvent.click(radio);
  expect(api.setCloseBehavior).toHaveBeenCalledExactlyOnceWith('tray');
  finish({ ...initial, action: 'tray' });
  await waitFor(() => expect(group).toHaveAttribute('aria-busy', 'false'));
  expect(radio).toBeChecked();
  expect(document.activeElement).toBe(radio);
});

it('is found by the alternative name "Tray" although the visible texts say notification area', async () => {
  vi.mocked(api.getCloseBehavior).mockResolvedValue({ action: 'exit', platform: 'windows', supported: true, trayAvailable: true });
  const tools = { query: 'tray', report: () => {}, reportSection: () => {} };
  const { container } = render(<LanguageProviderWithDiagnostics><SettingsToolsContext.Provider value={tools}><CloseBehaviorSettings /></SettingsToolsContext.Provider></LanguageProviderWithDiagnostics>);
  await screen.findAllByRole('radio');
  expect(container.querySelector('[data-setting-row]')).not.toHaveAttribute('hidden');
});

it('hides the row for a search that matches neither its texts nor its alternative names', async () => {
  vi.mocked(api.getCloseBehavior).mockResolvedValue({ action: 'exit', platform: 'windows', supported: true, trayAvailable: true });
  const tools = { query: 'zzzunmatched', report: () => {}, reportSection: () => {} };
  const { container } = render(<LanguageProviderWithDiagnostics><SettingsToolsContext.Provider value={tools}><CloseBehaviorSettings /></SettingsToolsContext.Provider></LanguageProviderWithDiagnostics>);
  await waitFor(() => expect(container.querySelector('[data-setting-row]')).toHaveAttribute('hidden'));
});
