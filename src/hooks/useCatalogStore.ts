import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import * as filesApi from '../lib/api/files';
import * as foldersApi from '../lib/api/folders';
import * as catalogMetaApi from '../lib/api/catalogMeta';
import { canonicalTag } from '../lib/autoTags';
import type { ModelFile, ModelFileSummary, Folder, TagCount, CreatorCount, SavedFilter, ImportResultDto } from '../types';

// Bettet eine schlanke `ModelFileSummary` (siehe Finding M-01) in die volle
// `ModelFile`-Form ein, damit die Grid-/Listen-Ansicht sowie alle
// bestehenden Komponenten/Hooks weiterhin denselben `ModelFile`-Typ sehen,
// ohne dass jede Stelle im Baum angepasst werden muss. Die hier NICHT von
// list_file_summaries gelieferten Felder (materials/tags/customImage/
// sliceInfo/costEstimate/sourceUrl/lastViewedAt) werden mit neutralen
// Defaults gefuellt und erst nachtraeglich per
// `ensureFullModel()`/`listFilesByIds([id])` echt befuellt, sobald ein
// Modell ausgewaehlt oder die Detailseite geoeffnet wird. renderSnapshotImage
// und creator kommen dagegen bereits direkt aus der Summary (Bugfix
// 2026-09-20 fuer beide - siehe Kommentar an `db::FileSummary`).
function summaryToModelFile(s: ModelFileSummary): ModelFile {
  return {
    id: s.id,
    name: s.name,
    path: s.path,
    folderId: s.folderId,
    tags: [],
    origin: 'local',
    sync: 'local-only',
    dimensionsMm: s.dimensionsMm,
    volumeCm3: s.volumeCm3,
    objectCount: s.objectCount,
    plateCount: null,
    materials: [],
    fileSizeBytes: s.fileSizeBytes,
    importedAt: s.importedAt,
    printStatus: s.printStatus,
    estimatedWeightG: null,
    weightSource: 'estimated',
    sliceInfo: null,
    costEstimate: null,
    lastViewedAt: null,
    creator: s.creator,
    customImage: null,
    thumbnailImage: s.thumbnailImage,
    // Bugfix (2026-09-20): list_file_summaries liefert render_snapshot_png
    // jetzt mit (siehe Kommentar an db::FileSummary) - vorher stand hier
    // hartcodiert `null`, wodurch das Grid einen im Hintergrund bereits
    // gerenderten Snapshot nie zeigte, bevor das Modell einzeln per
    // ensureFullModel nachgeladen wurde.
    renderSnapshotImage: s.renderSnapshotImage,
    sourceUrl: null,
    queuePosition: s.queuePosition,
    favorite: s.favorite,
    deletedAt: null,
  };
}

// Finding 3 (Abschluss-Review): Feld-Praefixe, die im "<feld>:<id>"-Schema
// von pendingMutationCounts/mutationEpochs oben verwendet werden, gemappt auf
// den tatsaechlichen ModelFile-Feldnamen (weicht bei "queue"/"renderSnapshot"
// vom Feldnamen ab). ensureFullModel() nutzt dieselbe Liste, um genau diese
// Felder aus einem parallel laufenden/gerade erst resyncten
// Mutations-Aufruf von seinem eigenen (potenziell veralteten) Fetch-Ergebnis
// auszunehmen - siehe Kommentar an ensureFullModel unten.
const MUTATION_TRACKED_FIELDS: { prefix: string; field: keyof ModelFile }[] = [
  { prefix: 'lastViewedAt', field: 'lastViewedAt' },
  { prefix: 'tags', field: 'tags' },
  { prefix: 'printStatus', field: 'printStatus' },
  { prefix: 'favorite', field: 'favorite' },
  { prefix: 'queue', field: 'queuePosition' },
  { prefix: 'renderSnapshot', field: 'renderSnapshotImage' },
  { prefix: 'sourceUrl', field: 'sourceUrl' },
];

export function useCatalogStore() {
  const [models, setModels] = useState<ModelFile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [tags, setTags] = useState<TagCount[]>([]);
  const [creators, setCreators] = useState<CreatorCount[]>([]);
  const [savedFilters, setSavedFilters] = useState<SavedFilter[]>([]);
  const [trashModels, setTrashModels] = useState<ModelFile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // IDs, fuer die bereits die volle ModelFile-Auskunft (Materials/Tags/
  // Bilder/Metadata) per listFilesByIds nachgeladen wurde - verhindert
  // wiederholtes Nachladen, solange kein refreshFiles() die Liste wieder auf
  // schlanke Summaries zuruecksetzt (siehe ensureFullModel).
  const [fullyLoadedIds, setFullyLoadedIds] = useState<Set<string>>(new Set());
  const [skippedSnapshotIds, setSkippedSnapshotIds] = useState<Set<string>>(new Set());
  // Finding 1 (Abschluss-Review): IDs, fuer die die zuletzt geladene Summary
  // bestaetigt hat, dass in der DB bereits ein render_snapshot_png existiert
  // (`ModelFileSummary.hasRenderSnapshot`). Der grosse Blob selbst wird ueber
  // list_file_summaries nie mitgeliefert, weshalb `renderSnapshotImage` fuer
  // summary-geladene Modelle IMMER `null` ist - ohne dieses Set wuerde
  // `pendingSnapshotIds` jedes Modell faelschlich als "braucht Snapshot"
  // werten, auch wenn bereits einer gespeichert ist (Re-Rendering + erneutes
  // Persistieren des GESAMTEN Katalogs bei jedem refreshFiles()).
  const [summaryConfirmedSnapshotIds, setSummaryConfirmedSnapshotIds] = useState<Set<string>>(new Set());
  // Pro Modell-ID statt global, damit ein Fehler/Erfolg von Modell A nicht
  // unter Modell B stehen bleibt, wenn der Nutzer zwischendurch die
  // Detailseite wechselt - kein separater Reset-Effekt nötig, da der Zugriff
  // in der UI immer gegen detailModel.id abgeglichen wird.
  const [rescanFeedback, setRescanFeedback] = useState<
    { fileId: string; status: 'success' | 'error'; message?: string } | null
  >(null);
  // Nur ein einmaliger "Wunsch", nach dem naechsten Commit zu diesem Modell
  // zu scrollen (siehe renameFile) - App.tsx setzt ihn per useEffect nach
  // Erledigung wieder auf null, kein dauerhafter UI-Zustand.
  const [pendingScrollToId, setPendingScrollToId] = useState<string | null>(null);

  // M-03: Rollback fuer fehlgeschlagene optimistische Mutationen (favorite/
  // printStatus/tags/sourceUrl/renderSnapshot/lastViewedAt). Weder ein
  // simples "bei Fehler auf `previous` zuruecksetzen" noch ein reiner
  // Generation-Zaehler ("nur der neueste Aufruf darf zurueckrollen") sind
  // hier korrekt - siehe Task-8-Brief fuer die durchgespielten Gegenbei-
  // spiele (u. a.: der jeweils neueste Aufruf kann selbst fehlschlagen,
  // waehrend ein noch aelterer, als "ueberholt" ignorierter Aufruf ebenso
  // fehlschlaegt, sodass die UI dauerhaft von einem Backend abweicht, das
  // nie einen erfolgreichen Schreibvorgang verzeichnet hat). Stattdessen:
  // pro "<feld>:<id>" wird nur die Anzahl GERADE LAUFENDER Backend-Aufrufe
  // gezaehlt (pendingMutationCounts). Faellt sie fuer ein Feld+ID wieder auf
  // 0 - d. h. alle bis dahin gestarteten ueberlappenden Aufrufe fuer genau
  // dieses Feld+ID sind abgeschlossen, unabhaengig von Erfolg/Fehler -, wird
  // der kanonische Datensatz einmalig per listFilesByIds([id]) (Task 6/M-01)
  // nachgeladen und uebernommen. Das ist korrekt per Konstruktion: nach
  // Abschluss aller Aufrufe fuer ein Feld+ID entspricht der Backend-Stand
  // immer der Wahrheit, unabhaengig davon, welcher einzelne Aufruf erfolg-
  // reich war oder wie die Aufrufe sich zeitlich ueberlappt haben.
  //
  // Ein zusaetzlicher monoton steigender Epoch-Zaehler pro Feld+ID
  // (mutationEpochs) verhindert daruber hinaus, dass ein spaet ankommender,
  // aber LAENGST VERALTETER Resync-Aufruf einen bereits abgeschlossenen,
  // neueren Resync-Aufruf fuer dasselbe Feld+ID wieder ueberschreibt (siebte
  // Review-Runde): ein reiner "pendingMutationCounts[key] === undefined"-
  // Check nach dem await reicht dafuer nicht, da der Zaehler zwischenzeitlich
  // erneut auf 0 gefallen sein kann, obwohl bereits ein aktuellerer Resync
  // gelaufen ist. Ein Resync uebernimmt sein Ergebnis deshalb nur, wenn beim
  // Abschluss seines awaits sowohl der Pending-Count als auch die Epoch fuer
  // dieses Feld+ID unveraendert gegenueber dem Start dieses Resyncs sind.
  //
  // Bewusst als Ref (nicht als State) gefuehrt: die Zaehler selbst loesen nie
  // direkt einen Re-Render aus, nur der daraus resultierende setModels()-
  // Aufruf tut das - ein State-Update pro beginMutation()/Zaehlerstand waere
  // hier unnoetig und wuerde zusaetzliche Re-Renders erzeugen.
  const pendingMutationCounts = useRef<Record<string, number>>({}).current;
  const mutationEpochs = useRef<Record<string, number>>({}).current;

  function beginMutation(key: string): void {
    pendingMutationCounts[key] = (pendingMutationCounts[key] ?? 0) + 1;
    mutationEpochs[key] = (mutationEpochs[key] ?? 0) + 1;
  }

  // Wird IMMER im .finally() der betroffenen Mutation aufgerufen, unabhaengig
  // von Erfolg oder Fehler. Generischer Helfer - arbeitet nur mit
  // key/id, kein feldspezifischer Code darin - und wird von JEDER
  // betroffenen Funktion unveraendert wiederverwendet (kein Copy-Paste pro
  // Feld).
  async function endMutationAndResyncIfSettled(key: string, id: string): Promise<void> {
    const remaining = (pendingMutationCounts[key] ?? 1) - 1;
    if (remaining > 0) {
      pendingMutationCounts[key] = remaining;
      return;
    }
    delete pendingMutationCounts[key];
    const epochAtResyncStart = mutationEpochs[key];
    try {
      const [fresh] = await filesApi.listFilesByIds([id]);
      // Nur uebernehmen, wenn WAEHREND des await oben (a) keine neue Mutation
      // fuer dieses Feld+ID gestartet wurde UND (b) die Epoch seit Start
      // dieses Resyncs unveraendert ist - (b) faengt genau den Fall ab, dass
      // zwischenzeitlich ein NEUERER Resync (fuer eine inzwischen bereits
      // wieder abgeschlossene Mutation) bereits seinerseits einen aktuelleren
      // Zustand uebernommen hat, waehrend dieser (aeltere) Resync noch laeuft.
      if (fresh && pendingMutationCounts[key] === undefined && mutationEpochs[key] === epochAtResyncStart) {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, ...fresh } : m)));
      }
    } catch (e) {
      console.error(`[resync] Nachladen von ${key} fehlgeschlagen:`, e);
      // Kein weiterer Rollback hier - der naechste reguläre refreshFiles()
      // gleicht spaetestens dann wieder ab.
    }
  }

  const pendingSnapshotIds = useMemo(
    () =>
      models
        .filter(
          (m) =>
            m.renderSnapshotImage === null &&
            !skippedSnapshotIds.has(m.id) &&
            !summaryConfirmedSnapshotIds.has(m.id),
        )
        .map((m) => m.id),
    [models, skippedSnapshotIds, summaryConfirmedSnapshotIds],
  );

  const refreshFolders = useCallback(() => foldersApi.listFolders().then(setFolders), []);

  // Laedt die schlanken Summaries UND die Datei->Tag-Zuordnung in je EINER
  // Abfrage (parallel) und mergt Tags client-seitig in die daraus
  // abgeleiteten ModelFile-Stubs - ein einziger zusaetzlicher Request fuer
  // die GESAMTE Liste, kein Pro-Zeile-Nachladen (Nachtrag zu Finding M-01:
  // ohne das faende die Sidebar-Tag-Filterung fuer noch nicht einzeln
  // geoeffnete Modelle keine Treffer mehr, siehe catalogFilters.ts).
  const loadSummariesWithTags = useCallback(() => {
    return Promise.all([filesApi.listFileSummaries(), filesApi.listAllFileTags()]).then(([summaries, tagsByFileId]) => {
      // Finding 1: das Praesenz-Flag aus der Summary ist der einzige Ort, an
      // dem "hat dieses Modell WIRKLICH schon einen Snapshot?" bekannt ist -
      // hier einmal pro Ladevorgang fuer die gesamte Liste einsammeln statt
      // `renderSnapshotImage === null` (das fuer JEDES summary-Modell zutrifft).
      setSummaryConfirmedSnapshotIds(new Set(summaries.filter((s) => s.hasRenderSnapshot).map((s) => s.id)));
      return summaries.map((s) => ({ ...summaryToModelFile(s), tags: tagsByFileId[s.id] ?? [] }));
    });
  }, []);

  // Katalog-Uebersicht laedt seit Finding M-01 nur noch die schlanke
  // Summary-Projektion statt der vollen ModelFile-Liste (Materials/
  // Metadata sowie render_snapshot_png/custom_image_png wurden bisher pro
  // Zeile mitgeladen, obwohl die Grid-/Listen-Ansicht sie gar nicht
  // anzeigt) - Tags werden ueber loadSummariesWithTags() separat in einer
  // einzigen Aggregat-Abfrage gemergt. Ein refreshFiles() setzt damit alle
  // Eintraege wieder auf den schlanken Stand zurueck - bereits
  // "hochgestufte" (fullyLoadedIds) Eintraege werden beim naechsten
  // ensureFullModel()-Aufruf einfach erneut nachgeladen, das ist unkritisch
  // (kein Datenverlust, nur ein zusaetzlicher Roundtrip).
  const refreshFiles = useCallback(() => {
    setFullyLoadedIds(new Set());
    return loadSummariesWithTags().then(setModels);
  }, [loadSummariesWithTags]);
  const refreshTags = useCallback(() => catalogMetaApi.listTagCounts().then(setTags), []);
  const refreshCreators = useCallback(() => catalogMetaApi.listCreators().then(setCreators), []);
  const refreshSavedFilters = useCallback(() => catalogMetaApi.listSavedFilters().then(setSavedFilters), []);
  const refreshTrash = useCallback(() => filesApi.listTrash().then(setTrashModels), []);

  // Laedt die vollen Modelldaten (Materials/Tags/Bilder/Metadata) fuer genau
  // ein Modell nach, sobald es tatsaechlich gebraucht wird (Auswahl in der
  // Liste oder Oeffnen der Detailseite) - siehe Task-6-Brief Step 4b/6.
  // Kein Einzeldatensatz-Command: `listFilesByIds` wird mit einer Liste der
  // Laenge 1 aufgerufen. Ersetzt den bisherigen Eintrag in `models` in-place,
  // damit `models.find(id)` (selected/detailModel in App.tsx) danach die
  // vollen Daten liefert, ohne dass Grid/Liste selbst umgebaut werden
  // muessen.
  const ensureFullModel = useCallback(
    (id: string) => {
      if (fullyLoadedIds.has(id) || !models.some((m) => m.id === id)) return;
      // Finding 3 (Abschluss-Review): Epoch-Schnappschuss aller mutations-
      // faehigen Felder VOR dem Fetch. ensureFullModel() ersetzte den
      // Modell-Datensatz bisher unbedingt vollstaendig - nahm dieser Fetch
      // laenger als eine parallel gestartete Mutation UND deren eigener
      // M-03-Resync, ueberschrieb das (dann veraltete) Fetch-Ergebnis den
      // bereits korrekt resyncten neuen Wert wieder mit dem alten Stand,
      // ohne dass die UI das je bemerkt haette. Der Schnappschuss erlaubt es,
      // beim Anwenden genau zu erkennen, fuer welche Felder waehrend des
      // Fetches etwas passiert ist.
      const epochSnapshot = MUTATION_TRACKED_FIELDS.map(({ prefix }) => mutationEpochs[`${prefix}:${id}`]);
      filesApi
        .listFilesByIds([id])
        .then((full) => {
          if (full.length === 0) return;
          setModels((prev) =>
            prev.map((m) => {
              if (m.id !== id) return m;
              const merged: ModelFile = { ...full[0] };
              MUTATION_TRACKED_FIELDS.forEach(({ prefix, field }, index) => {
                const key = `${prefix}:${id}`;
                const stillPending = pendingMutationCounts[key] !== undefined;
                const epochAdvanced = mutationEpochs[key] !== epochSnapshot[index];
                if (stillPending || epochAdvanced) {
                  // Fuer dieses Feld ist waehrend/nach dem Fetch eine
                  // Mutation gestartet oder resynct worden - das
                  // Fetch-Ergebnis ist fuer GENAU DIESES Feld potenziell
                  // veraltet, deshalb den aktuellen (bereits optimistischen
                  // oder frisch resyncten) Wert behalten statt ihn mit dem
                  // Fetch-Ergebnis zu ueberschreiben. Alle anderen Felder
                  // werden regulaer aus `full[0]` uebernommen.
                  (merged as unknown as Record<string, unknown>)[field] = m[field];
                }
              });
              return merged;
            }),
          );
          setFullyLoadedIds((prev) => new Set(prev).add(id));
        })
        .catch((e) => {
          console.error('[full-model] Nachladen fehlgeschlagen:', e);
        });
    },
    [models, fullyLoadedIds],
  );

  useEffect(() => {
    loadSummariesWithTags().then((mapped) => {
      setModels(mapped);
      setSelectedId((prev) => prev ?? mapped[0]?.id ?? null);
    });
    refreshFolders();
    refreshTags();
    refreshCreators();
    refreshSavedFilters();
    refreshTrash();
    // Nur beim Mount - die Refresher selbst sind stabil (useCallback ohne Deps).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const skipSnapshot = useCallback((id: string) => {
    setSkippedSnapshotIds((prev) => new Set(prev).add(id));
  }, []);

  const selectModel = useCallback(
    (id: string) => {
      setSelectedId(id);
      ensureFullModel(id);
      const now = new Date().toISOString();
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, lastViewedAt: now } : m)));
      const key = `lastViewedAt:${id}`;
      beginMutation(key);
      filesApi
        .markFileViewed(id)
        .catch((e) => {
          console.error('[last-viewed] Aktualisieren fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [ensureFullModel],
  );

  const mergeImported = useCallback(
    (result: ImportResultDto) => {
      if (result.imported.length) {
        setModels((prev) => [...prev, ...result.imported]);
        // Frisch importierte Dateien kommen bereits als volle ModelFile-
        // Datensaetze vom Import-Command - kein erneutes Nachladen noetig.
        setFullyLoadedIds((prev) => {
          const next = new Set(prev);
          result.imported.forEach((m) => next.add(m.id));
          return next;
        });
        setSelectedId(result.imported[result.imported.length - 1].id);
        refreshFolders();
        refreshTags();
        refreshCreators();
      }
    },
    [refreshFolders, refreshTags, refreshCreators],
  );

  const applyLocalDeletion = useCallback(
    (deletedIds: string[]) => {
      setModels((prev) => prev.filter((m) => !deletedIds.includes(m.id)));
      setSelectedId((prev) => (prev && deletedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    },
    [refreshFolders, refreshTags, refreshCreators, refreshTrash],
  );

  // Fuer Aufrufer, bei denen delete_files einzelne IDs stillschweigend
  // uebersprungen haben kann (siehe deleteSelectedCleanupFiles) - laedt
  // die Modell-Liste autoritativ neu statt lokal zu filtern.
  const refetchAfterPartialDelete = useCallback(
    (affectedIds: string[]) => {
      refreshFiles();
      setSelectedId((prev) => (prev && affectedIds.includes(prev) ? null : prev));
      refreshFolders();
      refreshTags();
      refreshCreators();
      refreshTrash();
    },
    [refreshFiles, refreshFolders, refreshTags, refreshCreators, refreshTrash],
  );

  const restoreModel = useCallback(
    (id: string) =>
      filesApi.restoreFile(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
        refreshFiles();
        refreshFolders();
        refreshTags();
        refreshCreators();
      }),
    [refreshFiles, refreshFolders, refreshTags, refreshCreators],
  );

  const deleteModelPermanently = useCallback(
    (id: string) =>
      filesApi.deleteFilePermanently(id).then(() => {
        setTrashModels((prev) => prev.filter((m) => m.id !== id));
        setSelectedId((prev) => (prev === id ? null : prev));
      }),
    [],
  );

  const emptyTrashAction = useCallback(() => filesApi.emptyTrash().then(() => setTrashModels([])), []);

  const addTag = useCallback(
    (id: string, rawTag: string) => {
      // Uebersetzte Namen automatischer Tags (z. B. "Multipart") auf die
      // Kennung abbilden, bevor optimistisch aktualisiert wird - sonst
      // blitzt kurz ein zweiter Tag auf, bis das Backend normalisiert hat.
      const tag = canonicalTag(rawTag);
      const current = models.find((m) => m.id === id);
      if (!current || current.tags.includes(tag)) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: [...m.tags, tag] } : m)));
      const key = `tags:${id}`;
      beginMutation(key);
      filesApi
        .addTag(id, tag)
        .then(refreshTags)
        .catch((e) => {
          console.error('[tags] Hinzufuegen fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models, refreshTags],
  );

  // Bewusst NICHT optimistisch (anders als addTag/removeTag): eine
  // Namenskollision im Zielverzeichnis ist ein haeufiger, fuer den Nutzer
  // wichtiger Fehlerfall (siehe rename_file-Validierung im Backend), den die
  // aufrufende UI direkt anzeigen soll, statt ihn nur still per naechstem
  // Resync zu kaschieren. Der lokale Zustand wird deshalb erst nach
  // erfolgreicher Backend-Bestaetigung aktualisiert; ein Fehler wird an den
  // Aufrufer durchgereicht (Promise-Rejection).
  const renameFile = useCallback((id: string, name: string) => {
    return filesApi.renameFile(id, name).then(() => {
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, name } : m)));
      // Ein neuer Name kann die Sortierposition des Modells verschieben
      // (Standard-Sortierung ist "Name") - ohne dies wuerde das gerade
      // umbenannte, weiterhin ausgewaehlte Modell aus dem sichtbaren Bereich
      // rutschen, ohne dass der Nutzer merkt, wohin. Bewusst NICHT per
      // requestAnimationFrame direkt hier ausgeloest: das laeuft ausserhalb
      // von Reacts Render-Zyklus und hat keine Garantie, dass der DOM-Knoten
      // bereits an seiner neuen (sortierten) Position committet wurde, wenn
      // der Frame feuert. Stattdessen wird nur der Scroll-Wunsch vermerkt;
      // der tatsaechliche Scroll passiert in einem useEffect in App.tsx, der
      // garantiert erst NACH dem zu diesem setModels() gehoerenden Commit
      // + Paint laeuft.
      setPendingScrollToId(id);
    });
  }, []);

  const removeTag = useCallback(
    (id: string, tag: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, tags: m.tags.filter((t) => t !== tag) } : m)));
      const key = `tags:${id}`;
      beginMutation(key);
      filesApi
        .removeTag(id, tag)
        .then(refreshTags)
        .catch((e) => {
          console.error('[tags] Entfernen fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models, refreshTags],
  );

  const deleteModel = useCallback(
    (id: string) => filesApi.deleteFile(id).then(() => applyLocalDeletion([id])),
    [applyLocalDeletion],
  );

  const togglePrintStatus = useCallback(
    (id: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      const next = current.printStatus === 'printed' ? 'not_printed' : 'printed';
      setModels((prev) =>
        prev.map((m) =>
          m.id === id ? { ...m, printStatus: next, queuePosition: next === 'printed' ? null : m.queuePosition } : m,
        ),
      );
      const key = `printStatus:${id}`;
      beginMutation(key);
      filesApi
        .setPrintStatus(id, next)
        .catch((e) => {
          console.error('[print-status] Aktualisieren fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models],
  );

  const toggleFavorite = useCallback(
    (id: string) => {
      const current = models.find((m) => m.id === id);
      if (!current) return;
      const next = !current.favorite;
      setModels((prev) => prev.map((m) => (m.id === id ? { ...m, favorite: next } : m)));
      const key = `favorite:${id}`;
      beginMutation(key);
      filesApi
        .setFavorite(id, next)
        .catch((e) => {
          console.error('[favorite] Aktualisieren fehlgeschlagen:', e);
        })
        .finally(() => {
          void endMutationAndResyncIfSettled(key, id);
        });
    },
    [models],
  );

  const addToQueue = useCallback((id: string) => {
    filesApi
      .addToQueue(id)
      .then((position) => {
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: position } : m)));
      })
      .catch((e) => console.error('[queue] Hinzufügen fehlgeschlagen:', e));
  }, []);

  const removeFromQueue = useCallback((id: string) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, queuePosition: null } : m)));
    const key = `queue:${id}`;
    beginMutation(key);
    filesApi
      .removeFromQueue(id)
      .catch((e) => {
        console.error('[queue] Entfernen fehlgeschlagen:', e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const reorderQueue = useCallback(
    (orderedIds: string[]) => {
      const updates: { fileId: string; position: number }[] = [];
      orderedIds.forEach((id, index) => {
        const position = index + 1;
        const current = models.find((m) => m.id === id);
        if (current && current.queuePosition !== position) {
          updates.push({ fileId: id, position });
        }
      });
      if (updates.length === 0) return;
      setModels((prev) =>
        prev.map((m) => {
          const index = orderedIds.indexOf(m.id);
          return index === -1 ? m : { ...m, queuePosition: index + 1 };
        }),
      );
      // Queue-Reorder betrifft immer mehrere Modelle gleichzeitig (ein
      // Batch-Update, keine einzelne Feld+ID-Mutation) - das Pending-Count-
      // Muster oben ist fuer genau EIN Feld+ID gedacht und wuerde hier nur
      // unnoetige Komplexitaet bringen. Ein voller refreshFiles() bei Fehler
      // erreicht dasselbe Ziel (Resync mit dem tatsaechlichen Backend-Stand)
      // fuer den gesamten betroffenen Reorder-Batch in einem Rutsch (siehe
      // Task-8-Brief: "hier reicht der bestehende Ansatz").
      filesApi.reorderQueue(updates).catch((e) => {
        console.error('[queue] Neusortierung fehlgeschlagen:', e);
        void refreshFiles();
      });
    },
    [models, refreshFiles],
  );

  const uploadCustomImage = useCallback((id: string) => {
    filesApi
      .uploadCustomImage(id)
      .then((customImage) => {
        if (customImage === null) return;
        setModels((prev) => prev.map((m) => (m.id === id ? { ...m, customImage } : m)));
      })
      .catch((e) => {
        console.error('[custom-image] Hochladen fehlgeschlagen:', e);
      });
  }, []);

  const captureRenderSnapshot = useCallback((id: string, base64: string) => {
    const renderSnapshotImage = `data:image/png;base64,${base64}`;
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, renderSnapshotImage } : m)));
    const key = `renderSnapshot:${id}`;
    beginMutation(key);
    filesApi
      .setRenderSnapshot(id, base64)
      .catch((e) => {
        console.error('[render-snapshot] Speichern fehlgeschlagen:', e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const setModelSourceUrl = useCallback((id: string, url: string | null) => {
    setModels((prev) => prev.map((m) => (m.id === id ? { ...m, sourceUrl: url } : m)));
    const key = `sourceUrl:${id}`;
    beginMutation(key);
    filesApi
      .setSourceUrl(id, url)
      .catch((e) => {
        console.error('[source-url] Speichern fehlgeschlagen:', e);
      })
      .finally(() => {
        void endMutationAndResyncIfSettled(key, id);
      });
  }, []);

  const rescanMetadata = useCallback((id: string) => {
    setRescanFeedback(null);
    filesApi
      .rescanFileMetadata(id)
      .then((updated) => {
        setModels((prev) => prev.map((m) => (m.id === id ? updated : m)));
        // rescan_file_metadata liefert bereits den vollen ModelFile-Datensatz.
        setFullyLoadedIds((prev) => new Set(prev).add(id));
        setRescanFeedback({ fileId: id, status: 'success' });
      })
      .catch((e) => {
        console.error('[rescan] Neu-Einlesen fehlgeschlagen:', e);
        setRescanFeedback({ fileId: id, status: 'error', message: String(e) });
      });
  }, []);

  return {
    models, setModels,
    folders, tags, creators, savedFilters, trashModels,
    selectedId, setSelectedId,
    skippedSnapshotIds, skipSnapshot, pendingSnapshotIds,
    rescanFeedback,
    pendingScrollToId, setPendingScrollToId,
    refreshFolders, refreshFiles, refreshTags, refreshCreators, refreshTrash,
    selectModel, mergeImported, ensureFullModel,
    restoreModel, deleteModelPermanently, emptyTrashAction,
    addTag, removeTag, renameFile, deleteModel,
    togglePrintStatus, toggleFavorite,
    addToQueue, removeFromQueue, reorderQueue,
    uploadCustomImage, captureRenderSnapshot, setModelSourceUrl,
    rescanMetadata,
    applyLocalDeletion, refetchAfterPartialDelete,
  };
}
