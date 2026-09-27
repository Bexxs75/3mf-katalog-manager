import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ModelPreview } from './ModelPreview';
import { makeModelFile } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// The real ModelViewer needs a WebGL context, which jsdom doesn't provide.
// A stub is enough here: this test is about which of the two is mounted,
// not about the 3D rendering itself.
vi.mock('./ModelViewer', () => ({
  ModelViewer: ({ fileId }: { fileId: string }) => <div data-testid="model-viewer">{fileId}</div>,
}));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.setItem('3mf-katalog-language', 'de');
});

function renderPreview(hasStepPreview: boolean, path = '/catalog/teil.step') {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'has_step_preview') return Promise.resolve(hasStepPreview);
    if (cmd === 'open_step_download') return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected invoke in this test: ${cmd}`));
  });
  const model = makeModelFile({ id: 'm1', path });
  render(
    <LanguageProvider>
      <ModelPreview model={model} needsSnapshot={false} onSnapshotCaptured={() => {}} />
    </LanguageProvider>,
  );
  return { model };
}

describe('ModelPreview', () => {
  it('shows the STEP hint instead of the viewer for a STEP file without STEP support', async () => {
    renderPreview(false);
    await waitFor(() => expect(screen.getByText('Keine 3D-Vorschau für STEP-Dateien')).toBeInTheDocument());
    expect(screen.queryByTestId('model-viewer')).not.toBeInTheDocument();
    // Only ModelViewer ever asks the backend for geometry; it never mounted here.
    expect(invoke).not.toHaveBeenCalledWith('get_model_geometry', expect.anything());
  });

  it('renders the real viewer for a STEP file when this build has STEP support', async () => {
    renderPreview(true);
    await waitFor(() => expect(screen.getByTestId('model-viewer')).toBeInTheDocument());
    expect(screen.queryByText('Keine 3D-Vorschau für STEP-Dateien')).not.toBeInTheDocument();
  });

  it('renders the real viewer for a non-STEP file even without STEP support', async () => {
    renderPreview(false, '/catalog/teil.3mf');
    await waitFor(() => expect(screen.getByTestId('model-viewer')).toBeInTheDocument());
  });

  it('the button opens the download page for the current UI language', async () => {
    renderPreview(false);
    await waitFor(() => screen.getByText('Variante mit STEP-Vorschau laden'));
    fireEvent.click(screen.getByText('Variante mit STEP-Vorschau laden'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('open_step_download', { lang: 'de' }));
  });
});
