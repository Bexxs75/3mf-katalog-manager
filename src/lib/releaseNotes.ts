import type { Language } from '../i18n/types';
import { version } from '../../package.json';

export const currentVersion = version;
export interface ReleaseGroups { new: string[]; improved: string[]; fixed: string[] }
export interface ReleaseNote { date: string; notes: Partial<Record<Language, ReleaseGroups>> & { en: ReleaseGroups } }
// 0.18.2 date is a placeholder to update at release.
export const releaseNotes: Record<string, ReleaseNote> = {
  "0.18.2": {
    "date": "2026-11-17",
    "notes": {
      "de": {
        "new": [
          "Das Farbschema Weiß ist wieder wählbar (Einstellungen → Darstellung).",
          "Dieses Fenster zeigt dir nach einem Update die Neuigkeiten."
        ],
        "improved": [],
        "fixed": []
      },
      "en": {
        "new": [
          "The White color scheme is available again (Settings → Appearance).",
          "This window shows you what’s new after an update."
        ],
        "improved": [],
        "fixed": []
      },
      "es": {
        "new": [
          "El esquema de color Blanco vuelve a estar disponible (Ajustes → Apariencia).",
          "Esta ventana muestra las novedades después de una actualización."
        ],
        "improved": [],
        "fixed": []
      },
      "fr": {
        "new": [
          "Le thème Blanc est de nouveau disponible (Paramètres → Apparence).",
          "Cette fenêtre présente les nouveautés après une mise à jour."
        ],
        "improved": [],
        "fixed": []
      }
    }
  },
  "0.18.1": {
    "date": "2026-11-16",
    "notes": {
      "de": {
        "new": [
          "Das Farbschema Hell heißt jetzt Sand.",
          "Das neue Farbschema Schiefer lässt sich manuell auswählen."
        ],
        "improved": [],
        "fixed": []
      },
      "en": {
        "new": [
          "The Light color scheme is now called Sand.",
          "The new Slate color scheme can be selected manually."
        ],
        "improved": [],
        "fixed": []
      },
      "es": {
        "new": [
          "El esquema de color Claro ahora se llama Arena.",
          "El nuevo esquema de color Pizarra se puede seleccionar manualmente."
        ],
        "improved": [],
        "fixed": []
      },
      "fr": {
        "new": [
          "Le thème Clair s’appelle désormais Sable.",
          "Le nouveau thème Ardoise peut être sélectionné manuellement."
        ],
        "improved": [],
        "fixed": []
      }
    }
  }
};
