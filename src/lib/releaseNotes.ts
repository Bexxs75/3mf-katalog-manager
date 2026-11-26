import type { Language } from '../i18n/types';
import { version } from '../../package.json';

export const currentVersion = version;
export interface ReleaseGroups { new: string[]; improved: string[]; fixed: string[] }
export interface ReleaseNote { date: string; notes: Partial<Record<Language, ReleaseGroups>> & { en: ReleaseGroups } }
export const releaseNotes: Record<string, ReleaseNote> = {
  "0.19.0": {
    "date": "2026-11-26",
    "notes": {
      "en": {
        "new": [
          "The web version catches up with the desktop app: import from the server's source folders, restoring a backup, printer connection, folder trash, disk comparison and cleanup, tar uploads, Save as STL and the STEP preview in the web image.",
          "A bundle can hold a single file, and a whole bundle can be moved to another folder (drag it or use Move to).",
          "Save as STL can also write ASCII STL, and Makerworld files get their source link when imported (only with a purely numeric model ID)."
        ],
        "improved": [
          "Rename, move, trash, folders, companion files, disk comparison and backup restore of the desktop app now run on the same core as the web version; nothing changes visibly.",
          "The web server is hardened: a password for large removals, quotas, a start password file instead of a log line, and trusted proxies must be listed explicitly.",
          "Desktop and web refuse concurrent operation on the same catalog."
        ],
        "fixed": [
          "Dragging files out of the catalog into Bambu Studio, OrcaSlicer and editors works on Linux again (known limits: releasing the mouse immediately can still start the drag; a cancel by a GTK3 target can freeze the window).",
          "The window remembers its size and maximized state (also on GNOME/Wayland), and the tray option is found in the settings search under \"Tray\".",
          "A narrow sidebar keeps the name of a missing folder readable; the disk comparison labels new folders as folders."
        ]
      },
      "de": {
        "new": [
          "Die Web-Version schließt zur Desktop-App auf: Import aus den Quellordnern des Servers, Sicherung einspielen, Druckeranbindung, Ordner-Papierkorb, Festplattenabgleich und Bereinigung, tar-Uploads, „Als STL speichern“ und die STEP-Vorschau im Web-Image.",
          "Ein Bündel darf eine einzelne Datei enthalten, und ein ganzes Bündel lässt sich in einen anderen Ordner verschieben (ziehen oder „Verschieben nach“).",
          "„Als STL speichern“ kann auch ASCII-STL schreiben, und Makerworld-Dateien bekommen beim Import ihren Quell-Link (nur bei rein numerischer Modell-ID)."
        ],
        "improved": [
          "Umbenennen, Verschieben, Papierkorb, Ordner, Begleitdateien, Festplattenabgleich und Sicherungs-Wiederherstellung der Desktop-App laufen jetzt auf demselben Kern wie die Web-Version; sichtbar ändert sich nichts.",
          "Der Web-Server ist gehärtet: Passwort bei großen Entfernungen, Obergrenzen, Startpasswort in einer Datei statt im Log, vertraute Proxys müssen ausdrücklich angegeben werden.",
          "Desktop und Web verweigern den gleichzeitigen Betrieb auf demselben Katalog."
        ],
        "fixed": [
          "Das Herausziehen von Dateien aus dem Katalog in Bambu Studio, OrcaSlicer und Editoren funktioniert unter Linux wieder (bekannte Grenzen: Loslassen sofort kann den Zug trotzdem starten; ein Abbruch durch ein GTK3-Ziel kann die Oberfläche einfrieren).",
          "Das Fenster merkt sich Größe und Maximiert-Zustand (auch unter GNOME/Wayland), und die Tray-Option wird in der Einstellungssuche unter „Tray“ gefunden.",
          "Eine schmale Seitenleiste lässt den Namen eines fehlenden Ordners lesbar; der Abgleich nennt neue Ordner „Ordner“."
        ]
      },
      "es": {
        "new": [
          "La versión web se acerca a la aplicación de escritorio: importación desde las carpetas de origen del servidor, restaurar una copia de seguridad, conexión con impresoras, papelera de carpetas, comparación con el disco y limpieza, subidas tar, Guardar como STL y la vista previa STEP en la imagen web.",
          "Un paquete puede contener un solo archivo y un paquete entero se puede mover a otra carpeta (arrastrarlo o usar Mover a).",
          "Guardar como STL también puede escribir STL ASCII, y los archivos de Makerworld reciben su enlace de origen al importarlos (solo con un ID de modelo puramente numérico)."
        ],
        "improved": [
          "Renombrar, mover, papelera, carpetas, archivos asociados, comparación con el disco y restauración de copias de la aplicación de escritorio usan ahora el mismo núcleo que la versión web; no cambia nada visible.",
          "El servidor web está reforzado: contraseña para eliminaciones grandes, límites, contraseña inicial en un archivo en lugar del registro y los proxies de confianza deben indicarse expresamente.",
          "La aplicación de escritorio y el servidor web impiden el uso simultáneo del mismo catálogo."
        ],
        "fixed": [
          "Arrastrar archivos fuera del catálogo hacia Bambu Studio, OrcaSlicer y editores vuelve a funcionar en Linux (límites conocidos: soltar el ratón de inmediato aún puede iniciar el arrastre; una cancelación por un destino GTK3 puede congelar la ventana).",
          "La ventana recuerda su tamaño y su estado maximizado (también en GNOME/Wayland), y la opción de la bandeja se encuentra en la búsqueda de ajustes con «Tray».",
          "Una barra lateral estrecha mantiene legible el nombre de una carpeta que falta; la comparación con el disco llama «carpeta» a las carpetas nuevas."
        ]
      },
      "fr": {
        "new": [
          "La version web rattrape l’application de bureau : import depuis les dossiers sources du serveur, restauration d’une sauvegarde, connexion aux imprimantes, corbeille de dossiers, comparaison avec le disque et nettoyage, envois tar, Enregistrer en STL et l’aperçu STEP dans l’image web.",
          "Un lot peut contenir un seul fichier, et un lot entier peut être déplacé vers un autre dossier (le glisser ou utiliser Déplacer vers).",
          "Enregistrer en STL peut aussi écrire du STL ASCII, et les fichiers Makerworld reçoivent leur lien source à l’import (uniquement avec un ID de modèle purement numérique)."
        ],
        "improved": [
          "Renommer, déplacer, corbeille, dossiers, fichiers associés, comparaison avec le disque et restauration de sauvegarde de l’application de bureau utilisent désormais le même noyau que la version web ; rien ne change visiblement.",
          "Le serveur web est renforcé : mot de passe pour les grandes suppressions, limites, mot de passe initial dans un fichier plutôt que dans le journal, et les proxys de confiance doivent être indiqués explicitement.",
          "L’application de bureau et le serveur web empêchent l’utilisation simultanée du même catalogue."
        ],
        "fixed": [
          "Le glisser de fichiers hors du catalogue vers Bambu Studio, OrcaSlicer et les éditeurs fonctionne de nouveau sous Linux (limites connues : relâcher la souris immédiatement peut quand même lancer le glisser ; une annulation par une cible GTK3 peut figer la fenêtre).",
          "La fenêtre mémorise sa taille et son état agrandi (aussi sous GNOME/Wayland), et l’option de la zone de notification se trouve dans la recherche des réglages sous « Tray ».",
          "Une barre latérale étroite garde lisible le nom d’un dossier manquant ; la comparaison avec le disque nomme « dossier » les nouveaux dossiers."
        ]
      }
    }
  },
  "0.18.7": {
    "date": "2026-11-22",
    "notes": {
      "en": {
        "new": [],
        "improved": [],
        "fixed": [
          "Minimize to tray works again: the window stays hidden instead of the app quitting.",
          "Ctrl+A in the log preview of the problem report selects only the log text."
        ]
      },
      "de": {
        "new": [],
        "improved": [],
        "fixed": [
          "„In den Tray minimieren“ funktioniert wieder: Das Fenster bleibt versteckt, statt dass die App beendet wird.",
          "Strg+A in der Protokoll-Vorschau des Problemberichts markiert nur den Protokolltext."
        ]
      },
      "es": {
        "new": [],
        "improved": [],
        "fixed": [
          "Minimizar a la bandeja funciona de nuevo: la ventana permanece oculta en lugar de cerrar la aplicación.",
          "Ctrl+A en la vista previa del registro del informe de problemas selecciona solo el texto del registro."
        ]
      },
      "fr": {
        "new": [],
        "improved": [],
        "fixed": [
          "La réduction dans la zone de notification fonctionne de nouveau : la fenêtre reste masquée au lieu de quitter l'application.",
          "Ctrl+A dans l'aperçu du journal du rapport de problème ne sélectionne que le texte du journal."
        ]
      }
    }
  },
  "0.18.6": {
    "date": "2026-11-22",
    "notes": {
      "en": {
        "new": [],
        "improved": [],
        "fixed": [
          "The What’s new window appears again after an update."
        ]
      },
      "de": {
        "new": [],
        "improved": [],
        "fixed": [
          "Das Fenster „Neuigkeiten“ erscheint nach einem Update wieder."
        ]
      },
      "es": {
        "new": [],
        "improved": [],
        "fixed": [
          "La ventana «Novedades» vuelve a aparecer después de una actualización."
        ]
      },
      "fr": {
        "new": [],
        "improved": [],
        "fixed": [
          "La fenêtre « Nouveautés » s’affiche de nouveau après une mise à jour."
        ]
      }
    }
  },
  "0.18.5": {
    "date": "2026-11-22",
    "notes": {
      "en": {
        "new": [
          "New app icon."
        ],
        "improved": [
          "Consistent wording: “Remove from catalog” keeps the file, “Move to trash” removes it.",
          "The help dialog lists F5, F2 and the sidebar keys; Enter confirms the restock popover; view buttons report their state to screen readers.",
          "Interface settings are saved in a file in the app data folder."
        ],
        "fixed": []
      },
      "de": {
        "new": [
          "Neues App-Icon."
        ],
        "improved": [
          "Einheitliche Begriffe: „Aus Katalog entfernen“ lässt die Datei liegen, „In den Papierkorb“ entfernt sie.",
          "Der Hilfedialog nennt F5, F2 und die Tasten der Seitenleiste; Enter bestätigt das Nachbestell-Fenster; Ansichtsknöpfe melden Screenreadern ihren Zustand.",
          "Die Oberflächen-Einstellungen werden in einer Datei im Datenordner der App gespeichert."
        ],
        "fixed": []
      },
      "es": {
        "new": [
          "Nuevo icono de la aplicación."
        ],
        "improved": [
          "Textos coherentes: «Quitar del catálogo» conserva el archivo, «Mover a la papelera» lo elimina.",
          "El diálogo de ayuda incluye F5, F2 y las teclas de la barra lateral; Intro confirma la ventana de reposición; los botones de vista indican su estado a los lectores de pantalla.",
          "Los ajustes de la interfaz se guardan en un archivo en la carpeta de datos de la aplicación."
        ],
        "fixed": []
      },
      "fr": {
        "new": [
          "Nouvelle icône de l’application."
        ],
        "improved": [
          "Libellés cohérents : « Retirer du catalogue » conserve le fichier, « Mettre à la corbeille » le supprime.",
          "La fenêtre d’aide mentionne F5, F2 et les touches de la barre latérale ; Entrée valide la fenêtre de réassort ; les boutons d’affichage indiquent leur état aux lecteurs d’écran.",
          "Les réglages de l’interface sont enregistrés dans un fichier du dossier de données de l’application."
        ],
        "fixed": []
      }
    }
  },
  "0.18.4": {
    "date": "2026-11-18",
    "notes": {
      "en": {
        "new": [
          "Apply nozzle and bed temperatures to all filament spools in a group when editing a spool."
        ],
        "improved": [],
        "fixed": [
          "Preview images rendered by older versions with flat lighting are rendered again once."
        ]
      },
      "de": {
        "new": [
          "Düsen- und Betttemperaturen beim Bearbeiten auf alle Filamentspulen einer Gruppe übertragen."
        ],
        "improved": [],
        "fixed": [
          "Vorschaubilder aus älteren Versionen mit flacher Beleuchtung werden einmal neu gerendert."
        ]
      },
      "es": {
        "new": [
          "Aplicar las temperaturas de boquilla y cama a todas las bobinas de filamento de un grupo al editar una bobina."
        ],
        "improved": [],
        "fixed": [
          "Las vistas previas generadas por versiones anteriores con iluminación plana se vuelven a generar una vez."
        ]
      },
      "fr": {
        "new": [
          "Appliquer les températures de buse et de plateau à toutes les bobines de filament d’un groupe lors de la modification."
        ],
        "improved": [],
        "fixed": [
          "Les aperçus générés par d’anciennes versions avec un éclairage plat sont régénérés une fois."
        ]
      }
    }
  },
  "0.18.3": {
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
  "0.18.2": {
    "date": "2026-11-17",
    "notes": {
      "de": { "new": ["Bei jeder Filamentspule lassen sich Düsentemperatur (von–bis) und optional die Druckbetttemperatur festhalten; die Lagertabelle zeigt die Düsentemperatur."], "improved": [], "fixed": [] },
      "en": { "new": ["Each filament spool can store a nozzle temperature range and, optionally, a bed temperature range; the inventory table shows the nozzle temperature."], "improved": [], "fixed": [] },
      "es": { "new": ["Cada bobina de filamento puede guardar un rango de temperatura de boquilla y, opcionalmente, de la cama; la tabla de inventario muestra la temperatura de boquilla."], "improved": [], "fixed": [] },
      "fr": { "new": ["Chaque bobine de filament peut enregistrer une plage de température de buse et, en option, du plateau ; le tableau du stock affiche la température de buse."], "improved": [], "fixed": [] }
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
