// Click on a queue entry: selects the model. If the detail page is open,
// it switches to the clicked model - the detail area on the right is hidden
// there, so a plain selection would stay invisible.
export function selectFromQueue(
  id: string,
  { detailOpen, selectModel, openDetail }: { detailOpen: boolean; selectModel: (id: string) => void; openDetail: (id: string) => void },
): void {
  selectModel(id);
  if (detailOpen) openDetail(id);
}
