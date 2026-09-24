// Klick auf einen Warteschlangen-Eintrag: waehlt das Modell aus. Ist gerade
// die Detailseite offen, wechselt sie zum angeklickten Modell - dort ist der
// Detailbereich rechts ausgeblendet, eine reine Auswahl bliebe unsichtbar.
export function selectFromQueue(
  id: string,
  { detailOpen, selectModel, openDetail }: { detailOpen: boolean; selectModel: (id: string) => void; openDetail: (id: string) => void },
): void {
  selectModel(id);
  if (detailOpen) openDetail(id);
}
