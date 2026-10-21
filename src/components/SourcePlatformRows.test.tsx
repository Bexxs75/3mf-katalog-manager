import type { ReactElement } from 'react';
import { render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { platformDisplayName } from '../lib/sourcePlatform';
import type { ModelFile } from '../types';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const noop = vi.fn();
const panel = (model: ModelFile) => (
  <LanguageProvider><UiDensityProvider><DetailPanel model={model} allTags={[]}
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop}
    onToggleFavorite={noop} onToggleQueue={noop} onUploadImage={noop}
    onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop} slicerError={null} /></UiDensityProvider></LanguageProvider>
);
const page = (model: ModelFile) => (
  <LanguageProvider><ModelDetailPage model={model} allTags={[]} onClose={noop} onAddTag={noop} onRemoveTag={noop}
    onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop} onToggleQueue={noop}
    onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    onRescanMetadata={noop} onAddToCollection={noop} collections={[]} slicers={[]} slicerError={null}
    rescanError={null} rescanSuccess={false} displayPreference="thumbnail" hasPrevious={false} hasNext={false} /></LanguageProvider>
);
const views: [string, string, (m: ModelFile) => ReactElement][] = [
  ['compact panel', 'compact', panel], ['comfort panel', 'comfort', panel], ['detail page', 'comfort', page],
];

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });

describe.each(views)('%s', (_name, density, ui) => {
  beforeEach(() => localStorage.setItem('3mf-katalog-density', density));

  it('shows platform pill, model ID and profile ID when a platform is set', () => {
    render(ui(makeModelFile({ sourcePlatform: 'makerworld', sourceModelId: 'USfa963547e39de4', sourceProfileId: '1067399466' })));
    expect(screen.getByText('Plattform')).toBeVisible();
    expect(screen.getByText('Makerworld')).toBeVisible();
    expect(screen.getByText('Modell-ID')).toBeVisible();
    expect(screen.getByText('USfa963547e39de4')).toHaveClass('font-mono');
    expect(screen.getByText('Profil-ID')).toBeVisible();
    expect(screen.getByText('1067399466')).toHaveClass('font-mono');
  });

  it('adds no rows without a platform', () => {
    render(ui(makeModelFile({ sourcePlatform: null, sourceModelId: 'orphan', sourceProfileId: 'orphan2' })));
    expect(screen.queryByText('Plattform')).toBeNull();
    expect(screen.queryByText('Modell-ID')).toBeNull();
    expect(screen.queryByText('Profil-ID')).toBeNull();
    expect(screen.queryByText('orphan')).toBeNull();
  });

  it('omits the profile ID row when it is empty', () => {
    render(ui(makeModelFile({ sourcePlatform: 'makerworld', sourceModelId: 'M1', sourceProfileId: null })));
    expect(screen.getByText('Modell-ID')).toBeVisible();
    expect(screen.queryByText('Profil-ID')).toBeNull();
  });

  it('shows an unknown platform key as plain text', () => {
    render(ui(makeModelFile({ sourcePlatform: 'newhub', sourceModelId: 'M1' })));
    expect(screen.getByText('newhub')).toBeVisible();
  });

  it('renders markup in values as text, not HTML', () => {
    const evil = '<img src=x onerror=alert(1)><b>x</b>';
    const { container } = render(ui(makeModelFile({ sourcePlatform: 'makerworld', sourceModelId: evil, sourceProfileId: '<i>p</i>' })));
    expect(screen.getByText(evil)).toBeVisible();
    expect(screen.getByText('<i>p</i>')).toBeVisible();
    expect(container.querySelector('img[src="x"]')).toBeNull();
    expect(container.querySelector('i')).toBeNull();
  });

  it('wraps long IDs instead of truncating them', () => {
    const long = 'A'.repeat(200);
    render(ui(makeModelFile({ sourcePlatform: 'makerworld', sourceModelId: long })));
    const el = screen.getByText(long);
    expect(el.className).toContain('break-all');
    expect(el.className).not.toContain('truncate');
  });
});

it.each([
  ['de', ['Plattform', 'Modell-ID', 'Profil-ID']],
  ['en', ['Platform', 'Model ID', 'Profile ID']],
  ['es', ['Plataforma', 'ID del modelo', 'ID del perfil']],
  ['fr', ['Plateforme', 'ID du modèle', 'ID du profil']],
])('labels the rows in %s', (language, labels) => {
  localStorage.setItem('3mf-katalog-language', language);
  localStorage.setItem('3mf-katalog-density', 'comfort');
  render(page(makeModelFile({ sourcePlatform: 'makerworld', sourceModelId: 'M1', sourceProfileId: 'P1' })));
  for (const label of labels) expect(screen.getByText(label)).toBeVisible();
  const row = screen.getByTestId('source-model-id-row');
  expect(within(row).getByText('M1')).toBeVisible();
});

it('maps known keys to display names and falls back to the key', () => {
  expect(platformDisplayName('makerworld')).toBe('Makerworld');
  expect(platformDisplayName('toString')).toBe('toString');
  expect(platformDisplayName('foo')).toBe('foo');
});
