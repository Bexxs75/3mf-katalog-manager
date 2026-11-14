import { isReadOnly } from '../lib/readOnly';
import { metadataBatch } from '../lib/metadataBatch';
import { reportMetadataError } from '../lib/metadataErrors';
import { canEditMetadata } from '../lib/webSession';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLanguage } from '../i18n/LanguageContext';
import * as collectionsApi from '../lib/api/collections';
import type { Collection, ModelFile } from '../types';

export function useCollections() {
  const { language } = useLanguage();
  const [collectionRecords, setCollections] = useState<Collection[]>([]);
  const collections = useMemo(() => {
    const collator = new Intl.Collator(language, { numeric: true, sensitivity: 'base' });
    return [...collectionRecords].sort((a, b) => collator.compare(a.name, b.name));
  }, [collectionRecords, language]);
  const [activeCollection, setActiveCollectionState] = useState<string | null>(null);
  const activeRef = useRef<string | null>(null);
  const request = useRef(0);
  const setActiveCollection = useCallback((id: string | null) => {
    activeRef.current = id;
    setActiveCollectionState(id);
  }, []);
  const [collectionsGalleryOpen, setCollectionsGalleryOpen] = useState(false);
  const [members, setMembers] = useState<{ id: string | null; models: ModelFile[] }>({ id: null, models: [] });
  const collectionModels = members.id === activeCollection ? members.models : [];
  const collectionLoading = activeCollection !== null && members.id !== activeCollection;

  const refreshCollections = useCallback(() => collectionsApi.listCollections().then(setCollections), []);

  const refreshCollectionModels = useCallback(
    async (collectionId: string) => {
      const ticket = ++request.current;
      try {
        const models = await collectionsApi.listCollectionFiles(collectionId);
        // Late responses must not replace the current view or a newer refresh.
        if (activeRef.current === collectionId && ticket === request.current) setMembers({ id: collectionId, models });
      } catch (error) {
        if (activeRef.current === collectionId && ticket === request.current) {
          setMembers(previous => previous.id === collectionId ? previous : { id: collectionId, models: [] });
        }
        throw error;
      }
    },
    [],
  );

  useEffect(() => {
    refreshCollections();
  }, [refreshCollections]);

  useEffect(() => {
    if (activeCollection) {
      void refreshCollectionModels(activeCollection).catch(error => console.error('[collections] loading failed:', error));
    } else {
      ++request.current;
      setMembers({ id: null, models: [] });
    }
  }, [activeCollection, refreshCollectionModels]);

  const reorderCollection = useCallback(
    (orderedIds: string[]) => {
      if (!canEditMetadata()) return Promise.resolve();
      if (!activeCollection) return;
      if (isReadOnly()) {
        const collectionId = activeCollection;
        return collectionsApi.reorderCollection(collectionId, orderedIds.map((fileId, position) => ({ fileId, position })))
          .catch(reportMetadataError)
          .then(() => refreshCollectionModels(collectionId)).catch(reportMetadataError);
      }
      const updates = orderedIds.map((fileId, position) => ({ fileId, position }));
      setMembers((prev) => {
        const byId = new Map(prev.models.map((m) => [m.id, m]));
        return { ...prev, models: orderedIds.map((id) => byId.get(id)).filter((m): m is ModelFile => m !== undefined) };
      });
      collectionsApi.reorderCollection(activeCollection, updates).catch((e) => {
        console.error('[collections] reordering failed:', e); reportMetadataError(e);
      });
    },
    [activeCollection],
  );

  const createCollection = useCallback(
    (name: string) => !canEditMetadata() ? Promise.resolve() : collectionsApi.createCollection(name).then(() => refreshCollections()).catch(error => { reportMetadataError(error); throw error; }),
    [refreshCollections],
  );

  const renameCollection = useCallback(
    (id: string, name: string) => !canEditMetadata() ? Promise.resolve() : collectionsApi.renameCollection(id, name).then(() => refreshCollections()).catch(error => { reportMetadataError(error); throw error; }),
    [refreshCollections],
  );

  const deleteCollection = useCallback(
    (id: string) =>
      !canEditMetadata() ? Promise.resolve() : collectionsApi.deleteCollection(id).then(() => {
        if (isReadOnly()) {
          if (activeCollection === id) setActiveCollection(null);
          return refreshCollections();
        }
        refreshCollections();
        if (activeCollection === id) setActiveCollection(null);
      }).catch(error => { reportMetadataError(error); throw error; }),
    [refreshCollections, activeCollection],
  );

  const addModelToCollection = useCallback(
    (fileId: string, collectionId: string) =>
      !canEditMetadata() ? Promise.resolve() : collectionsApi.addFilesToCollection(collectionId, [fileId]).then(async () => {
        await Promise.all([
          refreshCollections(),
          ...(activeRef.current === collectionId ? [refreshCollectionModels(collectionId)] : []),
        ]);
      }).catch(error => { reportMetadataError(error); throw error; }),
    [refreshCollections, refreshCollectionModels],
  );

  const bulkAddToCollection = useCallback(
    (fileIds: string[], collectionId: string) =>
      !canEditMetadata() ? Promise.resolve() : isReadOnly() ? metadataBatch(fileIds, id => collectionsApi.addFilesToCollection(collectionId, [id]), async () => {
        await refreshCollections();
        if (activeRef.current === collectionId) await refreshCollectionModels(collectionId);
      }) : collectionsApi.addFilesToCollection(collectionId, fileIds).then(async () => {
        await Promise.all([
          refreshCollections(),
          ...(activeRef.current === collectionId ? [refreshCollectionModels(collectionId)] : []),
        ]);
      }),
    [refreshCollections, refreshCollectionModels],
  );

  const bulkRemoveFromCollection = useCallback(
    (fileIds: string[]) => {
      if (!canEditMetadata()) return Promise.resolve();
      if (!activeCollection) return Promise.resolve();
      if (isReadOnly()) return metadataBatch(fileIds, id => collectionsApi.removeFileFromCollection(activeCollection, id), async () => {
        await refreshCollections(); await refreshCollectionModels(activeCollection);
      });
      return Promise.all(
        fileIds.map((fileId) => collectionsApi.removeFileFromCollection(activeCollection, fileId)),
      ).then(() => {
        refreshCollections();
        refreshCollectionModels(activeCollection);
      });
    },
    [activeCollection, refreshCollections, refreshCollectionModels],
  );

  return {
    collections,
    activeCollection,
    setActiveCollection,
    collectionsGalleryOpen,
    setCollectionsGalleryOpen,
    collectionModels,
    collectionLoading,
    refreshCollections,
    refreshCollectionModels,
    reorderCollection,
    createCollection,
    renameCollection,
    deleteCollection,
    addModelToCollection,
    bulkAddToCollection,
    bulkRemoveFromCollection,
  };
}
