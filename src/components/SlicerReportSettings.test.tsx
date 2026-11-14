import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { SlicerReportSettings } from './SlicerReportSettings';
import * as api from '../lib/api/slicerReports';

vi.mock('../lib/api/slicerReports');

it('keeps switch focus during and after a pending save', async () => {
  const initial = { enabled: false, programPath: null, pathChanged: false, commands: [], openCount: 0, lastError: null };
  vi.mocked(api.getSlicerReportSetup).mockResolvedValue(initial);
  let finish!: (value: typeof initial) => void;
  vi.mocked(api.setSlicerReportEnabled).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  render(<LanguageProviderWithDiagnostics><SlicerReportSettings /></LanguageProviderWithDiagnostics>);
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
  expect(api.setSlicerReportEnabled).toHaveBeenCalledTimes(1);
  finish({ ...initial, enabled: true });
  await waitFor(() => expect(toggle).not.toHaveAttribute('aria-busy'));
  expect(toggle).toHaveAttribute('aria-checked', 'true');
  expect(document.activeElement).toBe(toggle);
});
