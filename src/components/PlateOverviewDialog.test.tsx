import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PlateOverviewDialog } from './PlateOverviewDialog';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { makeModelFile, makePrintInfo } from '../test/factories';
import { de } from '../i18n/de';
import type { PlateOverview, SliceInfo } from '../types';
import * as filesApi from '../lib/api/files';

vi.mock('../lib/api/files', () => ({ readPlateOverview: vi.fn() }));
const readPlateOverview = vi.mocked(filesApi.readPlateOverview);

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  readPlateOverview.mockReset();
});

const png = 'data:image/png;base64,iVBORw0KGgo=';
const slice: SliceInfo = {
  totalWeightG: 52.8,
  plates: [
    { plateIndex: 1, weightG: 38.2, filaments: [] },
    { plateIndex: 2, weightG: 14.6, filaments: [] },
  ],
};
const plate = (plateIndex: number, printSeconds: number | null, name: string | null) => ({ plateIndex, name, printSeconds, firstLayerSeconds: null, plateBedType: null });
const info = makePrintInfo({
  printerModel: 'Bambu Lab X1 Carbon', layerHeightMm: 0.2, nozzleMm: { value: 0.4, mixed: false },
  plates: [plate(1, 8040, 'Gehäuse'), plate(2, 3120, null), plate(3, null, 'Halter')],
});
const model = makeModelFile({ id: 'm1', name: 'Box', path: '/lib/box.3mf', sliceInfo: slice, printInfo: info });
const overview: PlateOverview = { plates: [{ plateIndex: 1, objectCount: 2, preview: png }, { plateIndex: 2, objectCount: 1 }, { plateIndex: 3 }] };

function open(props: Partial<React.ComponentProps<typeof PlateOverviewDialog>> = {}) {
  const onClose = vi.fn();
  const view = render(<LanguageProvider><PlateOverviewDialog model={model} onClose={onClose} {...props} /></LanguageProvider>);
  return { onClose, ...view };
}
const rowOf = (name: RegExp | string) => screen.getByRole('row', { name });

describe('PlateOverviewDialog', () => {
  it('lists name, objects, time and weight per plate and sums only what is known', async () => {
    readPlateOverview.mockResolvedValue(overview);
    open();
    expect(readPlateOverview).toHaveBeenCalledWith('m1', true);
    const dialog = screen.getByRole('dialog', { name: new RegExp(de.poTitle) });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    await waitFor(() => expect(within(rowOf(/Gehäuse/)).getByText('2')).toBeVisible());
    const first = within(rowOf(/Gehäuse/));
    expect(first.getByText('2 h 14 min')).toBeVisible();
    expect(first.getByText(/38,2 g/)).toBeVisible();
    // A value the file does not give is a dash plus a spoken text, never a 0.
    const second = rowOf(/^\s*2\s/);
    expect(within(second).getAllByText(de.poNotGiven)).toHaveLength(1);
    const third = within(rowOf(/Halter/));
    expect(third.getAllByText(de.poNotGiven)).toHaveLength(3);
    expect(screen.queryByText(/^0 (g|min)$/)).toBeNull();
    // Totals: 3 objects known for 2 of 3 plates, so "at least"; time 2 h 14 + 52 min = 3 h 6 min of 2 of 3 plates.
    const total = within(screen.getByRole('row', { name: new RegExp(de.poTotal) }));
    expect(total.getByText('mind. 3')).toBeVisible();
    expect(total.getByText('mind. 3 h 6 min')).toBeVisible();
    expect(total.getByText(/mind\. 52,8 g/)).toBeVisible();
    expect(screen.getByText(de.poPartialNote)).toBeVisible();
  });

  it('says nothing about "at least" when every plate has every value', async () => {
    readPlateOverview.mockResolvedValue({ plates: [{ plateIndex: 1, objectCount: 2 }] });
    const single = makeModelFile({ id: 's', name: 'S', path: '/s.3mf', sliceInfo: { totalWeightG: 10, plates: [{ plateIndex: 1, weightG: 10, filaments: [] }] }, printInfo: makePrintInfo({ plates: [plate(1, 600, 'A')] }) });
    open({ model: single });
    await waitFor(() => expect(screen.getAllByText('2').length).toBeGreaterThan(0));
    expect(screen.queryByText(de.poPartialNote)).toBeNull();
    expect(screen.queryByText(/mind\./)).toBeNull();
  });

  it('shows the pictures on request and shows a neutral area where a plate has none', async () => {
    readPlateOverview.mockResolvedValue(overview);
    const { container } = open();
    await waitFor(() => expect(container.querySelectorAll('img')).toHaveLength(1));
    // Pictures are decoration: the plate name stands in the row.
    expect(container.querySelector('img')).toHaveAttribute('alt', '');
    expect(container.querySelector('img')).toHaveAttribute('src', png);
    fireEvent.click(screen.getByRole('checkbox', { name: de.poShowPreview }));
    expect(container.querySelectorAll('img')).toHaveLength(0);
    fireEvent.click(screen.getByRole('checkbox', { name: de.poShowPreview }));
    expect(container.querySelectorAll('img')).toHaveLength(1);
    expect(readPlateOverview).toHaveBeenCalledTimes(1);
  });

  it('switches between table and tiles', async () => {
    readPlateOverview.mockResolvedValue(overview);
    open();
    await waitFor(() => expect(screen.getByRole('table')).toBeVisible());
    const group = screen.getByRole('group', { name: de.poViewMode });
    expect(within(group).getByRole('radio', { name: de.poViewTable })).toBeChecked();
    fireEvent.click(within(group).getByRole('radio', { name: de.poViewTiles }));
    expect(screen.queryByRole('table')).toBeNull();
    expect(screen.getAllByRole('listitem')).toHaveLength(3);
    expect(screen.getByRole('heading', { name: /Platte 1: Gehäuse/ })).toBeVisible();
    expect(screen.getByText(new RegExp(`${de.poTotal}:`))).toBeVisible();
  });

  it('keeps name, time and weight when the file cannot be read, and says why', async () => {
    readPlateOverview.mockRejectedValue(new Error('gone'));
    open();
    await waitFor(() => expect(screen.getByText(de.poUnavailable)).toBeVisible());
    expect(screen.getByText(de.poUnavailable).closest('[role="status"]')).not.toBeNull();
    expect(within(rowOf(/Gehäuse/)).getByText('2 h 14 min')).toBeVisible();
    expect(within(rowOf(/Gehäuse/)).getByText(de.poNotGiven)).toBeVisible();
  });

  it('says so when the file holds no plate', async () => {
    readPlateOverview.mockResolvedValue({ plates: [] });
    open({ model: makeModelFile({ id: 'n', name: 'N', path: '/n.3mf', sliceInfo: null, printInfo: makePrintInfo({ plates: [] }) }) });
    await waitFor(() => expect(screen.getByText(de.poNoPlates)).toBeVisible());
    expect(screen.queryByRole('table')).toBeNull();
  });

  it('waits for the full record while only the list summary is there', () => {
    readPlateOverview.mockReturnValue(new Promise(() => {}));
    open({ model: makeModelFile({ id: 'p', name: 'P', path: '/p.3mf', summaryOnly: true }) });
    expect(screen.getByRole('status')).toHaveTextContent(de.poLoading);
    expect(screen.queryByRole('table')).toBeNull();
  });

  it('offers to read an unread file, only with a handler', async () => {
    readPlateOverview.mockResolvedValue({ plates: [] });
    const unread = makeModelFile({ id: 'u', name: 'U', path: '/u.3mf', sliceInfo: null, printInfo: null });
    const onReread = vi.fn();
    const { unmount } = open({ model: unread, onReread });
    expect(screen.getByText(de.poUnread)).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: de.poReread }));
    expect(onReread).toHaveBeenCalledWith('u');
    unmount();
    open({ model: unread });
    expect(screen.getByText(de.poUnread)).toBeVisible();
    expect(screen.queryByRole('button', { name: de.poReread })).toBeNull();
  });

  it('takes the focus on opening, closes on Escape and gives the focus back', async () => {
    readPlateOverview.mockResolvedValue(overview);
    function Host() {
      const [shown, setShown] = useState(false);
      return <LanguageProvider><button onClick={() => setShown(true)}>opener</button>{shown && <PlateOverviewDialog model={model} onClose={() => setShown(false)} />}</LanguageProvider>;
    }
    render(<Host />);
    screen.getByRole('button', { name: 'opener' }).focus();
    fireEvent.click(screen.getByRole('button', { name: 'opener' }));
    expect(screen.getByRole('button', { name: de.poClose })).toHaveFocus();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('button', { name: 'opener' })).toHaveFocus();
  });
});
