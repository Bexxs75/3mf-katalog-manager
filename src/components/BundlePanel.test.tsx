import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { BundlePanel, type BundleViewActions } from './BundlePanel';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { buildCards } from '../lib/bundles';
import { panelMembers } from '../lib/bundlePanel';
import { makeModelFile } from '../test/factories';
import type { Bundle, BundleKind, ModelFile } from '../types';

vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
afterEach(cleanup);

const plateTime = (seconds: number, complete = true) => ({ plateCount: 1, printSeconds: seconds, ...(complete ? { printSecondsComplete: true as const } : {}) });
function fixture(kind: BundleKind) {
  const models: ModelFile[] = [
    makeModelFile({ id: 'a', name: 'Wall 04.3mf', fileType: '3mf', tags: ['home', 'holder'], license: 'CC-BY-4.0', estimatedWeightG: 38, printStatus: 'printed',
      materials: [{ name: 'PLA', displayColor: null }], printFacts: plateTime(6720), fileSizeBytes: 2_400_000 }),
    makeModelFile({ id: 'b', name: 'Wall 06.3mf', fileType: '3mf', tags: ['home'], license: 'CC-BY-NC-4.0', estimatedWeightG: 36, printStatus: 'not_printed',
      materials: [{ name: 'PETG', displayColor: null }], printFacts: plateTime(4440), fileSizeBytes: 2_300_000 }),
    makeModelFile({ id: 'c', name: 'Wall raw.stl', fileType: 'stl', tags: ['holder', 'home'], license: 'CC-BY-4.0', printStatus: 'not_printed', queuePosition: 2, fileSizeBytes: 5_800_000 }),
  ];
  const bundle: Bundle = { id: 'b1', name: 'Wall holder', kind, createdAt: '2026-11-01T00:00:00Z', members: [
    { fileId: 'a', position: 0, label: '0.4 mm nozzle', inTrash: false }, { fileId: 'b', position: 1, label: null, inTrash: false }, { fileId: 'c', position: 2, label: 'Raw', inTrash: false }] };
  const card = buildCards(models, [bundle], { collapse: true })[0];
  if (card.type !== 'bundle') throw new Error('expected a bundle card');
  return { models, bundle, card };
}

function setup(kind: BundleKind, extra: { actions?: Partial<BundleViewActions>; onSetLicense?: boolean } = {}) {
  const { models, bundle, card } = fixture(kind);
  const handlers = {
    onOpenMember: vi.fn(), onSetPrinted: vi.fn(), onSetFavorite: vi.fn(), onAddTag: vi.fn(), onQueue: vi.fn(),
    onSetLicense: vi.fn().mockResolvedValue(undefined),
  };
  const base = { onEdit: vi.fn(), onDissolve: vi.fn(), onRemoveMember: vi.fn() };
  // The ask state lives in the caller; a tiny host keeps it like the app does.
  function Host() {
    const [ask, setAsk] = useState<string | null>(null);
    const actions: BundleViewActions = { ...base, queueAskBundleId: ask, setQueueAskBundleId: setAsk, ...extra.actions };
    return <LanguageProvider><BundlePanel card={card} members={panelMembers(bundle, models)} allTags={['home', 'holder', 'other']}
      displayPreference="thumbnail" actions={actions} onOpenMember={handlers.onOpenMember} onSetPrinted={handlers.onSetPrinted}
      onSetFavorite={handlers.onSetFavorite} onAddTag={handlers.onAddTag} onSetLicense={extra.onSetLicense === false ? undefined : handlers.onSetLicense}
      onQueue={handlers.onQueue} /></LanguageProvider>;
  }
  render(<Host />);
  return { ...handlers, ...base, models };
}

describe('BundlePanel content', () => {
  it('names the bundle, its kind and title model, and lists every member with label, status and a Title badge', () => {
    setup('variants');
    expect(screen.getByRole('heading', { level: 2, name: 'Wall holder' })).toBeInTheDocument();
    expect(screen.getByText('Bündel · Varianten')).toBeInTheDocument();
    expect(screen.getByText(/Titelmodell: 0.4 mm nozzle/)).toBeInTheDocument();
    const list = screen.getByRole('list', { name: de.bundleMembers });
    const items = within(list).getAllByRole('listitem');
    expect(items).toHaveLength(3);
    expect(within(items[0]).getByText('0.4 mm nozzle')).toBeInTheDocument();
    expect(within(items[0]).getByText(de.bundleDlgTitleBadge)).toBeInTheDocument();
    expect(within(items[0]).getByText(de.printStatusPrinted)).toBeInTheDocument();
    expect(within(items[1]).getByText('Wall 06.3mf')).toBeInTheDocument();
    expect(within(items[2]).getByText(de.printStatusNotPrinted)).toBeInTheDocument();
  });

  it('opens a member and the title model by their buttons', () => {
    const { onOpenMember } = setup('variants');
    fireEvent.click(screen.getByRole('button', { name: /Wall 06.3mf/ }));
    expect(onOpenMember).toHaveBeenLastCalledWith('b');
    fireEvent.click(screen.getByRole('button', { name: de.bundleOpenTitle }));
    expect(onOpenMember).toHaveBeenLastCalledWith('a');
  });

  it('shows the union of tags, "mixed" licenses and the printed count', () => {
    setup('variants');
    const summary = screen.getByRole('region', { name: de.bundleSummary });
    expect(within(summary).getAllByRole('listitem').map(item => item.textContent)).toEqual(['#holder', '#home']);
    expect(within(summary).getByText(de.bundleLicenseMixed)).toBeInTheDocument();
    expect(within(summary).getByText('1 von 3 gedruckt')).toBeInTheDocument();
  });

  it('adds up print time and weight for parts, saying "at least" when a part lacks the value', () => {
    setup('parts');
    const summary = screen.getByRole('region', { name: de.bundleSummary });
    // The STL has neither a time nor a weight, so both sums are incomplete.
    expect(within(summary).getByText(de.bundleAtLeast.replace('{value}', '3 h 6 min'))).toBeInTheDocument();
    expect(within(summary).getByText(/mindestens 74/)).toBeInTheDocument();
  });

  it('does not add up variants: time and weight stay with the members', () => {
    setup('variants');
    const summary = screen.getByRole('region', { name: de.bundleSummary });
    expect(within(summary).getAllByText(de.bundlePerVariant)).toHaveLength(2);
    expect(within(summary).queryByText(/mindestens/)).toBeNull();
  });
});

describe('BundlePanel actions', () => {
  it('marks the members that are not printed yet, announces it, and turns the button around', () => {
    const { onSetPrinted } = setup('parts');
    fireEvent.click(screen.getByRole('button', { name: 'Alle 3 als gedruckt markieren' }));
    expect(onSetPrinted).toHaveBeenCalledOnce();
    expect(onSetPrinted.mock.calls[0][0].map((model: ModelFile) => model.id)).toEqual(['b', 'c']);
    expect(onSetPrinted.mock.calls[0][1]).toBe(true);
    expect(screen.getByRole('status')).toHaveTextContent('Alle 3 als gedruckt markiert.');
  });

  it('marks all as favorites', () => {
    const { onSetFavorite } = setup('parts');
    fireEvent.click(screen.getByRole('button', { name: 'Alle 3 als Favoriten markieren' }));
    expect(onSetFavorite.mock.calls[0][0].map((model: ModelFile) => model.id)).toEqual(['a', 'b', 'c']);
    expect(onSetFavorite.mock.calls[0][1]).toBe(true);
  });

  it('adds a tag only to the members that lack it, and gives focus back to its button', () => {
    const { onAddTag } = setup('parts');
    const opener = screen.getByRole('button', { name: de.bundleApplyTag });
    fireEvent.click(opener);
    expect(opener).toHaveAttribute('aria-expanded', 'true');
    const input = screen.getByPlaceholderText(de.addTagPlaceholder);
    fireEvent.change(input, { target: { value: 'holder' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAddTag).toHaveBeenCalledOnce();
    expect(onAddTag.mock.calls[0][0].map((model: ModelFile) => model.id)).toEqual(['b']);
    expect(onAddTag.mock.calls[0][1]).toBe('holder');
    expect(screen.getByRole('status')).toHaveTextContent('Tag „holder“ zu allen 3 Mitgliedern hinzugefügt.');
    expect(opener).toHaveFocus();
  });

  it('sets the license for every member', async () => {
    const { onSetLicense } = setup('parts');
    fireEvent.click(screen.getByRole('button', { name: de.bundleApplyLicense }));
    fireEvent.click(screen.getByRole('button', { name: de.licenseSave }));
    await screen.findByText('Lizenz für alle 3 Mitglieder gesetzt.');
    expect(onSetLicense).toHaveBeenCalledWith(['a', 'b', 'c'], null);
  });

  it('closes an open editor with Escape and returns focus to its button', () => {
    setup('parts');
    const opener = screen.getByRole('button', { name: de.bundleApplyTag });
    fireEvent.click(opener);
    const input = screen.getByPlaceholderText(de.addTagPlaceholder);
    // The first Escape closes the suggestion list, the second the editor (the field's own rule).
    fireEvent.keyDown(input, { key: 'Escape' });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByPlaceholderText(de.addTagPlaceholder)).toBeNull();
    expect(opener).toHaveFocus();
  });

  it('passes edit and dissolve on to the app, with the pressed button as the focus target', () => {
    const { onEdit, onDissolve } = setup('parts');
    const edit = screen.getByRole('button', { name: de.bundleEdit });
    fireEvent.click(edit);
    expect(onEdit).toHaveBeenCalledWith('b1', edit);
    fireEvent.click(screen.getByRole('button', { name: de.bundleDissolve }));
    expect(onDissolve).toHaveBeenCalledWith('b1');
  });
});

describe('BundlePanel queue', () => {
  it('asks "Which variant?" for variants and queues only the chosen one', () => {
    const { onQueue } = setup('variants');
    const opener = screen.getByRole('button', { name: de.bundleQueueOpen });
    fireEvent.click(opener);
    const group = screen.getByRole('group', { name: de.bundleQueueWhich });
    const radios = within(group).getAllByRole('radio');
    expect(radios).toHaveLength(3);
    expect(radios[0]).toHaveFocus();
    expect(radios[0]).toBeChecked();
    // A variant that is queued already says so and cannot be queued twice.
    fireEvent.click(radios[2]);
    expect(within(group).getByText(de.bundleQueueIn)).toBeInTheDocument();
    expect(within(group).getByRole('button', { name: de.bundleQueueAddVariant })).toBeDisabled();
    fireEvent.click(radios[1]);
    fireEvent.click(within(group).getByRole('button', { name: de.bundleQueueAddVariant }));
    expect(onQueue).toHaveBeenCalledOnce();
    expect(onQueue.mock.calls[0][0].map((model: ModelFile) => model.id)).toEqual(['b']);
    expect(screen.getByRole('status')).toHaveTextContent('1 Modell zur Warteschlange hinzugefügt.');
    expect(screen.queryByRole('group', { name: de.bundleQueueWhich })).toBeNull();
    expect(opener).toHaveFocus();
  });

  it('cancels the question with the button and with Escape', () => {
    const { onQueue } = setup('variants');
    const opener = screen.getByRole('button', { name: de.bundleQueueOpen });
    fireEvent.click(opener);
    fireEvent.click(screen.getByRole('button', { name: de.cancel }));
    expect(screen.queryByRole('group', { name: de.bundleQueueWhich })).toBeNull();
    expect(opener).toHaveFocus();
    fireEvent.click(opener);
    fireEvent.keyDown(screen.getByRole('group', { name: de.bundleQueueWhich }), { key: 'Escape' });
    expect(screen.queryByRole('group', { name: de.bundleQueueWhich })).toBeNull();
    expect(onQueue).not.toHaveBeenCalled();
  });

  it('queues all parts that are not in the queue yet', () => {
    const { onQueue } = setup('parts');
    fireEvent.click(screen.getByRole('button', { name: de.bundleQueueOpen }));
    expect(screen.getByText('Bei Teilen kommen alle 3 Teile in die Warteschlange.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Alle 3 zur Warteschlange hinzufügen' }));
    expect(onQueue.mock.calls[0][0].map((model: ModelFile) => model.id)).toEqual(['a', 'b']);
    expect(screen.getByRole('status')).toHaveTextContent('2 Modelle zur Warteschlange hinzugefügt.');
  });

  it('opens the question when the app asks for it (card menu)', () => {
    setup('variants', { actions: { queueAskBundleId: 'b1' } });
    expect(screen.getByRole('group', { name: de.bundleQueueWhich })).toBeInTheDocument();
  });
});
