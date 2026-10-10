import type { ReactNode } from 'react';
import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { LanguageProvider } from '../i18n/LanguageContext';
import { useSlicers } from '../hooks/useSlicers';
import { useSlicerLauncher } from '../hooks/useSlicerLauncher';
import { useUpdater } from '../hooks/useUpdater';
import { RevealFileButton } from './RevealFileButton';
import { ContextMenu } from './ContextMenu';
import { UpdatePanel } from './UpdatePanel';
import { NetworkFilesystemWarning } from './NetworkFilesystemWarning';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock('../lib/api/update', () => ({ getAppVersion: () => Promise.resolve('0.16.0'), isPreviewBuild: () => Promise.resolve(false) }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(), info: vi.fn() }));
const wrapper = ({ children }: { children: ReactNode }) => <LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}>{children}</RuntimeEnvironmentProvider></LanguageProvider>;
beforeEach(() => { vi.mocked(invoke).mockReset().mockResolvedValue([]); localStorage.setItem('3mf-katalog-language', 'de'); });

it('does not detect, register or launch slicers in a container', async () => {
  const setup = vi.fn();
  const { result } = renderHook(() => ({ registry: useSlicers(), launcher: useSlicerLauncher([], null, setup) }), { wrapper });
  await act(async () => { await result.current.registry.addSlicer(); result.current.launcher.openInSlicer('42'); });
  expect(invoke).not.toHaveBeenCalled();
  expect(setup).not.toHaveBeenCalled();
});

it('hides reveal and context menu host actions', () => {
  render(<><RevealFileButton fileId="42" /><ContextMenu x={0} y={0} fileId="42" currentName="cube.stl"
    onClose={vi.fn()} onOpenInSlicer={vi.fn()} onDelete={vi.fn()} inQueue={false} onToggleQueue={vi.fn()}
    printed={false} onTogglePrintStatus={vi.fn()} onRename={vi.fn()} /></>, { wrapper });
  expect(screen.queryByRole('button', { name: 'In Slicer öffnen' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Im Dateimanager anzeigen' })).not.toBeInTheDocument();
});

it('keeps the version but never checks, downloads or installs updates', async () => {
  const { result } = renderHook(useUpdater, { wrapper });
  await waitFor(() => expect(result.current.currentVersion).toBe('0.16.0'));
  act(() => { result.current.checkNow(); result.current.startUpdate(); result.current.install(); result.current.retry(); });
  expect(invoke).not.toHaveBeenCalled();
  render(<UpdatePanel view={result.current} onExport={vi.fn()} />, { wrapper });
  expect(screen.getByText('0.16.0')).toBeInTheDocument();
  expect(screen.getByText('docker compose pull').tagName).toBe('CODE');
  expect(screen.getByText('docker compose up -d').tagName).toBe('CODE');
  expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
  expect(screen.getAllByRole('button')).toEqual([
    screen.getByRole('button', { name: 'Link kopieren: Was ist neu?' }),
  ]);
  expect(screen.getByText('https://github.com/Bexxs75/3mf-katalog-manager/releases/tag/v0.16.0')).toBeVisible();
});

it.each([false, true])('shows a dismissible network warning only when the marker exists (%s)', async exists => {
  vi.mocked(invoke).mockResolvedValue(exists);
  const { unmount } = render(<NetworkFilesystemWarning />, { wrapper });
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('has_network_filesystem_warning'));
  if (exists) {
    expect(await screen.findByRole('alert')).toHaveTextContent('SQLite');
    fireEvent.click(screen.getByRole('button', { name: 'Schließen' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    unmount();
    render(<NetworkFilesystemWarning />, { wrapper });
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  } else expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('does not query the warning marker on desktop', () => {
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: false }}><NetworkFilesystemWarning /></RuntimeEnvironmentProvider></LanguageProvider>);
  expect(invoke).not.toHaveBeenCalled();
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
