import { describe, expect, it, vi } from 'vitest';
import { selectFromQueue } from './queueSelect';

describe('selectFromQueue', () => {
  it('selects the model when no detail page is open', () => {
    const selectModel = vi.fn();
    const openDetail = vi.fn();
    selectFromQueue('m2', { detailOpen: false, selectModel, openDetail });
    expect(selectModel).toHaveBeenCalledWith('m2');
    expect(openDetail).not.toHaveBeenCalled();
  });

  it('switches the open detail page to the clicked model', () => {
    const selectModel = vi.fn();
    const openDetail = vi.fn();
    selectFromQueue('m2', { detailOpen: true, selectModel, openDetail });
    expect(selectModel).toHaveBeenCalledWith('m2');
    expect(openDetail).toHaveBeenCalledWith('m2');
  });
});
