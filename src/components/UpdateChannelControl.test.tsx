import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UpdateChannelControl } from './UpdateChannelControl';
import * as api from '../lib/api/updater';
vi.mock('../lib/api/updater', () => ({ getUpdateChannel: vi.fn(), setUpdateChannel: vi.fn(), discardAppUpdate: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));
beforeEach(() => { vi.clearAllMocks(); vi.mocked(api.getUpdateChannel).mockResolvedValue('stable'); vi.mocked(api.setUpdateChannel).mockResolvedValue(); vi.mocked(api.discardAppUpdate).mockResolvedValue(); });
function setup() {
  const onExport = vi.fn().mockResolvedValue(true), onChanged = vi.fn();
  render(<LanguageProvider><UpdateChannelControl disabled={false} onExport={onExport} onChanged={onChanged} /></LanguageProvider>);
  return { onExport, onChanged };
}
describe('RC channel consent', () => {
  it('requires explicit consent and cancellation leaves stable selected', async () => {
    setup();
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(api.setUpdateChannel).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('Abbrechen'));
    expect(toggle).not.toBeChecked();
  });
  it('opens existing export without silently enabling RC', async () => {
    const { onExport } = setup();
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    fireEvent.click(screen.getByText('Katalog jetzt exportieren'));
    await waitFor(() => expect(onExport).toHaveBeenCalledOnce());
    expect(api.setUpdateChannel).not.toHaveBeenCalled();
  });
  it('keeps the dialog open during export, then enables activation without another warning', async () => {
    const { onExport } = setup();
    let finish!: (saved: boolean) => void;
    onExport.mockImplementation(() => new Promise<boolean>(resolve => { finish = resolve; }));
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    fireEvent.click(screen.getByText('Katalog jetzt exportieren'));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText('Trotzdem aktivieren')).toBeDisabled();
    await act(async () => finish(true));
    expect(within(screen.getByRole('dialog')).getByText('Katalog exportiert')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Jetzt aktivieren'));
    await waitFor(() => expect(toggle).toBeChecked());
  });
  it('shows export errors inside the dialog and cancel leaves the channel off', async () => {
    const { onExport } = setup();
    onExport.mockRejectedValue(new Error('export failed'));
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    fireEvent.click(screen.getByText('Katalog jetzt exportieren'));
    await waitFor(() => expect(within(screen.getByRole('dialog')).getByRole('alert')).toHaveTextContent('export failed'));
    expect(screen.queryByText('Jetzt aktivieren')).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('Abbrechen'));
    expect(toggle).not.toBeChecked();
    expect(api.setUpdateChannel).not.toHaveBeenCalled();
  });
  it('does not claim success when export returns false', async () => {
    const { onExport } = setup();
    onExport.mockResolvedValue(false);
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    fireEvent.click(screen.getByText('Katalog jetzt exportieren'));
    await waitFor(() => expect(within(screen.getByRole('dialog')).getByRole('alert')).toBeInTheDocument());
    expect(screen.queryByText('Katalog exportiert')).not.toBeInTheDocument();
    expect(toggle).not.toBeChecked();
  });
  it('explains disabling without offering a downgrade', async () => {
    vi.mocked(api.getUpdateChannel).mockResolvedValue('rc');
    setup();
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).toBeChecked());
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(screen.getByRole('status')).toHaveTextContent('bis eine neuere stabile Version verfügbar ist');
    expect(api.setUpdateChannel).toHaveBeenCalledWith('stable');
  });
  it('keeps stable selected when saving fails', async () => {
    vi.mocked(api.setUpdateChannel).mockRejectedValue(new Error('save failed'));
    const { onChanged } = setup();
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    fireEvent.click(screen.getByText('Trotzdem aktivieren'));
    await screen.findByText('save failed');
    expect(toggle).not.toBeChecked();
    expect(onChanged).not.toHaveBeenCalled();
  });
  it('persists consent and rechecks the selected channel', async () => {
    const { onChanged } = setup();
    const toggle = await screen.findByRole('checkbox');
    await waitFor(() => expect(toggle).not.toBeDisabled());
    fireEvent.click(toggle);
    fireEvent.click(screen.getByText('Trotzdem aktivieren'));
    await waitFor(() => expect(toggle).toBeChecked());
    expect(api.setUpdateChannel).toHaveBeenCalledWith('rc');
    expect(onChanged).toHaveBeenCalledOnce();
  });
});
