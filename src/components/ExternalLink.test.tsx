import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { ExternalLink } from './ExternalLink';

const url = 'https://example.com/report?a=1&b=2';
const open = vi.fn();
function show(container = true) {
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container }}>
    <ExternalLink url={url} label="Report" onOpen={open} />
  </RuntimeEnvironmentProvider></LanguageProvider>);
}
beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); open.mockReset(); });
it('copies the selectable URL, announces success and retains focus without opening', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  show();
  expect(screen.queryByRole('link')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Report' })).not.toBeInTheDocument();
  expect(screen.getByText(url)).toHaveStyle({ userSelect: 'text' });
  const button = screen.getByRole('button', { name: 'Link kopieren: Report' });
  button.focus(); fireEvent.click(button);
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Link kopiert'));
  expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite');
  expect(writeText).toHaveBeenCalledWith(url);
  expect(button).toHaveFocus(); expect(open).not.toHaveBeenCalled();
});
it('keeps the URL selectable when copying fails', async () => {
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
  show(); fireEvent.click(screen.getByRole('button'));
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Bitte den Link markieren'));
  expect(screen.getByText(url)).toBeInTheDocument(); expect(open).not.toHaveBeenCalled();
});
it('uses the existing desktop action', () => {
  show(false); fireEvent.click(screen.getByRole('button', { name: 'Report' }));
  expect(open).toHaveBeenCalledOnce(); expect(screen.queryByText(url)).not.toBeInTheDocument();
});

it('announces a rejected clipboard write and allows another attempt', async () => {
  const writeText = vi.fn().mockRejectedValueOnce(new Error('denied')).mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  show(); const button = screen.getByRole('button'); button.focus(); fireEvent.click(button);
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Bitte den Link markieren'));
  expect(button).toHaveFocus(); expect(screen.getByText(url)).toBeVisible();
  fireEvent.click(button);
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Link kopiert'));
  expect(button).toHaveFocus(); expect(open).not.toHaveBeenCalled();
});
