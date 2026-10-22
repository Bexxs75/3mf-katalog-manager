import React from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DiagnosticsProvider } from '../diagnostics/DiagnosticsContext';
import { LinuxMenuSettings } from './LinuxMenuSettings';
import { LinuxMenuPrompt } from './LinuxMenuPrompt';
import { de } from '../i18n/de';
import * as api from '../lib/api/linuxMenu';

vi.mock('../lib/api/linuxMenu', async (importActual) => ({
  ...(await importActual<typeof import('../lib/api/linuxMenu')>()),
  getLinuxMenuStatus: vi.fn(),
  setLinuxMenuEnabled: vi.fn(),
  answerLinuxMenuPrompt: vi.fn(),
}));

const base: api.LinuxMenuStatus = { available: true, installed: false, thirdParty: false, conflict: false, conflictPath: null, shouldPrompt: false };

function wrap(node: React.ReactNode) {
  return render(<LanguageProvider><DiagnosticsProvider>{node}</DiagnosticsProvider></LanguageProvider>);
}

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.getLinuxMenuStatus).mockReset();
  vi.mocked(api.setLinuxMenuEnabled).mockReset();
  vi.mocked(api.answerLinuxMenuPrompt).mockReset();
});

describe('normalizeLinuxMenuStatus', () => {
  it('treats anything but an available status as not available', async () => {
    const { normalizeLinuxMenuStatus } = await vi.importActual<typeof import('../lib/api/linuxMenu')>('../lib/api/linuxMenu');
    expect(normalizeLinuxMenuStatus([])).toBeNull();
    expect(normalizeLinuxMenuStatus(null)).toBeNull();
    expect(normalizeLinuxMenuStatus({ available: false })).toBeNull();
    expect(normalizeLinuxMenuStatus({ available: true, installed: true })).toMatchObject({ installed: true, thirdParty: false });
  });
});

describe('LinuxMenuSettings', () => {
  it('shows nothing when the feature is not available', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(null);
    const { container } = wrap(<LinuxMenuSettings />);
    await waitFor(() => expect(api.getLinuxMenuStatus).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });

  it('adds the entry and shows the login hint', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(base);
    vi.mocked(api.setLinuxMenuEnabled).mockResolvedValue({ ...base, installed: true });
    wrap(<LinuxMenuSettings />);
    const toggle = await screen.findByRole('switch', { name: de.linuxMenuTitle });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
    expect(api.setLinuxMenuEnabled).toHaveBeenCalledWith(true);
    expect(screen.getByRole('status')).toHaveTextContent(de.linuxMenuAdded);
  });

  it('removes the entry again', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, installed: true });
    vi.mocked(api.setLinuxMenuEnabled).mockResolvedValue(base);
    wrap(<LinuxMenuSettings />);
    const toggle = await screen.findByRole('switch', { name: de.linuxMenuTitle });
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'false'));
    expect(api.setLinuxMenuEnabled).toHaveBeenCalledWith(false);
  });

  it('is disabled and explains itself when another tool integrates the AppImage', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, thirdParty: true });
    wrap(<LinuxMenuSettings />);
    expect(await screen.findByRole('switch', { name: de.linuxMenuTitle })).toBeDisabled();
    expect(screen.getByText(de.linuxMenuThirdParty)).toBeVisible();
  });

  it('names the foreign file and what to do about it', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, conflict: true, conflictPath: '/home/u/.local/share/applications/mf-katalog-manager.desktop' });
    wrap(<LinuxMenuSettings />);
    expect(await screen.findByRole('switch', { name: de.linuxMenuTitle })).toBeDisabled();
    expect(screen.getByText(de.linuxMenuForeignFilePath.replace('{path}', '/home/u/.local/share/applications/mf-katalog-manager.desktop'))).toBeVisible();
  });

  it('shows the heading once', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(base);
    wrap(<LinuxMenuSettings />);
    await screen.findByRole('switch', { name: de.linuxMenuTitle });
    expect(screen.getAllByText(de.linuxMenuTitle)).toHaveLength(1);
  });

  it('shows the translated message when a foreign file is in the way', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(base);
    vi.mocked(api.setLinuxMenuEnabled).mockRejectedValue({ message: 'linuxMenuForeignFile', expected: true });
    wrap(<LinuxMenuSettings />);
    fireEvent.click(await screen.findByRole('switch', { name: de.linuxMenuTitle }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.linuxMenuForeignFile);
  });
});

describe('StrictMode', () => {
  const strict = (node: React.ReactNode) => wrap(<React.StrictMode>{node}</React.StrictMode>);

  it('still shows the switch of the settings section', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(base);
    strict(<LinuxMenuSettings />);
    expect(await screen.findByRole('switch', { name: de.linuxMenuTitle })).toBeInTheDocument();
  });

  it('still shows the first-start question', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, shouldPrompt: true });
    strict(<LinuxMenuPrompt />);
    expect(await screen.findByRole('dialog', { name: de.linuxMenuPromptTitle })).toBeInTheDocument();
  });
});

describe('LinuxMenuPrompt', () => {
  it('stays closed unless the backend asks for it', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue(base);
    wrap(<LinuxMenuPrompt />);
    await waitFor(() => expect(api.getLinuxMenuStatus).toHaveBeenCalled());
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('writes nothing until "Ja" and then shows the hint', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, shouldPrompt: true });
    vi.mocked(api.answerLinuxMenuPrompt).mockResolvedValue({ ...base, installed: true });
    wrap(<LinuxMenuPrompt />);
    await screen.findByRole('dialog', { name: de.linuxMenuPromptTitle });
    expect(api.setLinuxMenuEnabled).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: de.linuxMenuYes }));
    expect(await screen.findByRole('status')).toHaveTextContent(de.linuxMenuAdded);
    expect(screen.getByRole('dialog', { name: de.linuxMenuAddedTitle })).toBeVisible();
    expect(api.answerLinuxMenuPrompt).toHaveBeenCalledWith('yes');
    // The answer button is gone; focus must land on the new close button, not on the page.
    expect(screen.getByRole('button', { name: de.impClose })).toHaveFocus();
  });

  it('does not ask while another first-run dialog is open, and asks once it is done', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, shouldPrompt: true });
    const { rerender } = wrap(<LinuxMenuPrompt wait />);
    await new Promise(r => setTimeout(r, 20));
    expect(api.getLinuxMenuStatus).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).toBeNull();
    rerender(<LanguageProvider><DiagnosticsProvider><LinuxMenuPrompt wait={false} /></DiagnosticsProvider></LanguageProvider>);
    await screen.findByRole('dialog', { name: de.linuxMenuPromptTitle });
  });

  it.each([['linuxMenuNotNow', 'not_now'], ['linuxMenuNever', 'never']] as const)('"%s" closes the dialog', async (key, answer) => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, shouldPrompt: true });
    vi.mocked(api.answerLinuxMenuPrompt).mockResolvedValue(base);
    wrap(<LinuxMenuPrompt />);
    fireEvent.click(await screen.findByRole('button', { name: de[key] }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(api.answerLinuxMenuPrompt).toHaveBeenCalledWith(answer);
  });

  it('keeps the dialog open and shows the error when saving fails', async () => {
    vi.mocked(api.getLinuxMenuStatus).mockResolvedValue({ ...base, shouldPrompt: true });
    vi.mocked(api.answerLinuxMenuPrompt).mockRejectedValue({ message: 'read-only file system', expected: true });
    wrap(<LinuxMenuPrompt />);
    fireEvent.click(await screen.findByRole('button', { name: de.linuxMenuYes }));
    expect(await screen.findByRole('alert')).toHaveTextContent('read-only file system');
    expect(screen.getByRole('dialog')).toBeVisible();
  });
});
