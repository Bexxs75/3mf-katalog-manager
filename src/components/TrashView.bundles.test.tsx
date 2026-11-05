import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ModelLayoutProvider } from '../hooks/ModelLayoutContext';
import { TrashView } from './TrashView';
import { makeModelFile } from '../test/factories';
import type { Bundle } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(false) }));
vi.mock('./ModelPreview', () => ({ ModelPreview: () => null }));
vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));
beforeEach(() => {
  localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de');
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ top: 0, left: 0, width: 800, height: 100, right: 800, bottom: 100 } as DOMRect);
});
afterEach(() => vi.restoreAllMocks());

const gear = makeModelFile({ id: 'g1', name: 'Gear.3mf', deletedAt: '2026-10-04T10:00:00Z' });
const plain = makeModelFile({ id: 'p1', name: 'Plain.3mf', deletedAt: '2026-10-04T10:00:00Z' });
const bundle: Bundle = {
  id: 'b1', name: 'Gearbox', kind: 'parts', createdAt: '2026-11-01T00:00:00Z',
  members: [
    { fileId: 'g1', position: 1, label: 'Lid', inTrash: true },
    { fileId: 'living', position: 0, label: null, inTrash: false },
  ],
};

function trash(view: 'grid' | 'groupedList', selectedId: string | null, withBundles = true) {
  return render(<LanguageProvider><UiDensityProvider><ModelLayoutProvider>
    <TrashView trashModels={[gear, plain]} selectedId={selectedId} onSelect={vi.fn()} view={view} confirmEmptyTrash={false}
      onConfirmEmptyTrashChange={vi.fn()} onEmptyTrash={vi.fn()} onRestore={vi.fn()} onDeletePermanently={vi.fn()}
      displayPreference="thumbnail" bundles={withBundles ? [bundle] : undefined} />
  </ModelLayoutProvider></UiDensityProvider></LanguageProvider>);
}

describe.each(['grid', 'groupedList'] as const)('trash (%s)', view => {
  it('keeps every file on its own and says which bundle a file was part of', () => {
    trash(view, null);
    const rows = [...document.querySelectorAll('[role="row"][data-model-id]')];
    expect(rows.map(row => row.getAttribute('data-model-id'))).toEqual(['g1', 'p1']);
    expect(document.querySelector('[data-bundle-id]')).toBeNull();
    const marked = rows[0] as HTMLElement;
    expect(within(marked).getByText('war Teil von Bündel „Gearbox“')).toBeInTheDocument();
    expect(marked).toHaveAccessibleName('Gear.3mf, war Teil von Bündel „Gearbox“');
    expect(rows[1].querySelector('[data-part-of]')).toBeNull();
  });
});

describe('trash detail panel', () => {
  it('names the bundle, its kind and the label of the file', () => {
    trash('grid', 'g1');
    const line = document.querySelector('[data-was-part-of]') as HTMLElement;
    expect(line).toHaveTextContent('war Teil von Bündel „Gearbox“');
    expect(line).toHaveTextContent('Teile · Lid');
  });

  it('says nothing for a file that was in no bundle', () => {
    trash('grid', 'p1');
    expect(document.querySelector('[data-was-part-of]')).toBeNull();
  });

  it('works without any bundle list', () => {
    trash('grid', 'g1', false);
    expect(document.querySelector('[data-was-part-of]')).toBeNull();
    expect(document.querySelector('[data-part-of]')).toBeNull();
  });
});
