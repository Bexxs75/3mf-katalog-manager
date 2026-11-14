import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
import { BundleBar, BundleDetailSection } from './BundleDetail';
import type { BundleViewActions } from './BundlePanel';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
import { panelMembers } from '../lib/bundlePanel';
import { makeModelFile } from '../test/factories';
import type { Bundle, BundleKind } from '../types';

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
afterEach(cleanup);

const models = [
  makeModelFile({ id: 'a', name: 'Gear housing', estimatedWeightG: 64, weightSource: 'slicer', printStatus: 'printed', materials: [{ name: 'PLA', displayColor: null }],
    printFacts: { plateCount: 1, printSeconds: 14400, printSecondsComplete: true } }),
  makeModelFile({ id: 'b', name: 'Gear wheel', estimatedWeightG: 18, printStatus: 'not_printed', materials: [{ name: 'PETG', displayColor: null }],
    printFacts: { plateCount: 1, printSeconds: 3300, printSecondsComplete: true } }),
];
function bundleOf(kind: BundleKind): Bundle {
  return { id: 'b1', name: 'Gears', kind, createdAt: '2026-11-01T00:00:00Z', members: [
    { fileId: 'a', position: 0, label: 'Housing', inTrash: false }, { fileId: 'b', position: 1, label: null, inTrash: false }] };
}

function bar(kind: BundleKind, currentId = 'a') {
  const bundle = bundleOf(kind);
  const onOpenMember = vi.fn();
  render(<LanguageProvider><BundleBar bundle={bundle} members={panelMembers(bundle, models)} currentId={currentId} onOpenMember={onOpenMember} /></LanguageProvider>);
  return onOpenMember;
}

describe('BundleBar for variants', () => {
  it('shows one button per variant, marks the open one and switches on a click', () => {
    const onOpen = bar('variants');
    const group = screen.getByRole('group', { name: de.bundleSwitchHeading });
    const [housing, wheel] = within(group).getAllByRole('button');
    expect(housing).toHaveAttribute('aria-pressed', 'true');
    expect(wheel).toHaveAttribute('aria-pressed', 'false');
    expect(housing).toHaveTextContent('Housing');
    expect(wheel).toHaveTextContent('Gear wheel');
    fireEvent.click(housing);
    expect(onOpen).not.toHaveBeenCalled();
    fireEvent.click(wheel);
    expect(onOpen).toHaveBeenCalledWith('b');
  });
});

describe('BundleBar for parts', () => {
  it('lists the parts with their values and totals them', () => {
    bar('parts');
    const table = screen.getByRole('table', { name: de.bundlePartsCaption });
    const rows = within(table).getAllByRole('row');
    // header, two parts, footer
    expect(rows).toHaveLength(4);
    expect(within(rows[1]).getByRole('button', { name: '1. Housing' })).toHaveAttribute('aria-current', 'true');
    expect(within(rows[1]).getByText('4 h')).toBeInTheDocument();
    expect(within(rows[2]).getByText('≈ 18 g')).toBeInTheDocument();
    const total = rows[3];
    expect(within(total).getByText('4 h 55 min')).toBeInTheDocument();
    expect(within(total).getByText('82 g')).toBeInTheDocument();
    expect(within(total).getByText('PLA, PETG')).toBeInTheDocument();
    expect(within(total).getByText('1 von 2 gedruckt')).toBeInTheDocument();
  });

  it('opens another part by its button', () => {
    const onOpen = bar('parts');
    fireEvent.click(screen.getByRole('button', { name: '2. Gear wheel' }));
    expect(onOpen).toHaveBeenCalledWith('b');
  });

  it('puts focus on the current entry of the next page after a switch (the page is rebuilt)', () => {
    const bundle = bundleOf('parts');
    function Host() {
      const [id, setId] = useState('a');
      // Like the app: a new key builds the page of the other member from scratch.
      return <LanguageProvider><BundleBar key={id} bundle={bundle} members={panelMembers(bundle, models)} currentId={id} onOpenMember={setId} /></LanguageProvider>;
    }
    render(<Host />);
    fireEvent.click(screen.getByRole('button', { name: '2. Gear wheel' }));
    expect(screen.getByRole('button', { name: '2. Gear wheel' })).toHaveFocus();
  });

  it('does not grab focus on a first render', () => {
    bar('parts');
    expect(document.body).toHaveFocus();
  });
});

describe('BundleDetailSection', () => {
  function section(extra: Partial<BundleViewActions> = {}) {
    const bundle = bundleOf('variants');
    const onOpenMember = vi.fn();
    const actions: BundleViewActions = { onEdit: vi.fn(), onDissolve: vi.fn(), onRemoveMember: vi.fn(), queueAskBundleId: null, setQueueAskBundleId: vi.fn(), ...extra };
    render(<LanguageProvider><BundleDetailSection bundle={bundle} members={panelMembers(bundle, models)} currentId="a" onOpenMember={onOpenMember} actions={actions} /></LanguageProvider>);
    return { onOpenMember, actions };
  }

  it('says which bundle the file is part of, marks the open file and jumps to the others', () => {
    const { onOpenMember } = section();
    expect(screen.getByText('Teil von „Gears“ · Varianten')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Housing/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Housing/ })).toHaveAttribute('aria-current', 'true');
    fireEvent.click(screen.getByRole('button', { name: 'Öffnen: Gear wheel' }));
    expect(onOpenMember).toHaveBeenCalledWith('b');
  });

  it('edits the bundle and removes the open file from it', () => {
    const { actions } = section();
    const edit = screen.getByRole('button', { name: de.bundleEdit });
    fireEvent.click(edit);
    expect(actions.onEdit).toHaveBeenCalledWith('b1', edit);
    fireEvent.click(screen.getByRole('button', { name: de.bundleRemoveFrom }));
    expect(actions.onRemoveMember).toHaveBeenCalledWith('a');
  });

  it('states that notes belong to the file', () => {
    section();
    expect(screen.getByText(de.bundleSectionNotes)).toBeInTheDocument();
  });
});
