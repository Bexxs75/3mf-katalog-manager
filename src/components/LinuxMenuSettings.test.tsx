import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { LinuxMenuSettings } from './LinuxMenuSettings';
import * as api from '../lib/api/linuxMenu';

vi.mock('../lib/api/linuxMenu');

it('keeps switch focus during and after a pending save', async () => {
  const initial = { available: true, installed: false, thirdParty: false, conflict: false, conflictPath: null, shouldPrompt: false };
  vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(initial);
  let finish!: (value: typeof initial) => void;
  vi.mocked(api.setLinuxMenuEnabled).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  render(<LanguageProviderWithDiagnostics><LinuxMenuSettings /></LanguageProviderWithDiagnostics>);
  const toggle = await screen.findByRole('switch');
  await waitFor(() => expect(toggle).toBeEnabled());
  toggle.focus();
  fireEvent.keyDown(toggle, { key: ' ' });
  expect(toggle).toHaveAttribute('aria-busy', 'true');
  expect(toggle).toHaveAttribute('aria-disabled', 'true');
  expect(toggle).toBeEnabled();
  expect(document.activeElement).toBe(toggle);
  fireEvent.click(toggle);
  fireEvent.keyDown(toggle, { key: 'Enter' });
  expect(api.setLinuxMenuEnabled).toHaveBeenCalledTimes(1);
  finish({ ...initial, installed: true });
  await waitFor(() => expect(toggle).not.toHaveAttribute('aria-busy'));
  expect(toggle).toHaveAttribute('aria-checked', 'true');
  expect(document.activeElement).toBe(toggle);
});
