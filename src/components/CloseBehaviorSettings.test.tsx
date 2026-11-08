import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { CloseBehaviorSettings } from './CloseBehaviorSettings';
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
