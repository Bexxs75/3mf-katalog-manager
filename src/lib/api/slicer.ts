import { invoke } from '@tauri-apps/api/core';

export interface SlicerDto {
  id: string;
  name: string;
  executablePath: string;
}

// M-06 (Task 11): der native Datei-Dialog fuer die Slicer-Auswahl laeuft
// jetzt vollstaendig im Backend (`pick_and_register_slicer`) - das
// Frontend uebergibt hier an keiner Stelle einen selbst konstruierten
// Pfad-String, einzige Quelle fuer einen neuen `executablePath`-Wert ist
// die vom Nutzer im (backend-gesteuerten) Dialog getroffene Auswahl.
export function pickAndRegisterSlicer() {
  return invoke<SlicerDto | null>('pick_and_register_slicer');
}

export function listRegisteredSlicers() {
  return invoke<SlicerDto[]>('list_registered_slicers');
}

// `open_in_slicer` nimmt seit Task 11 keinen freien Pfad mehr entgegen,
// sondern ausschliesslich die id eines zuvor registrierten Slicers - die
// eigentliche Pfad-Aufloesung/-Validierung passiert serverseitig gegen die
// `registered_slicers`-Registry.
export function openInSlicer(modelId: string, slicerId: string) {
  return invoke('open_in_slicer', { fileId: modelId, slicerId });
}
