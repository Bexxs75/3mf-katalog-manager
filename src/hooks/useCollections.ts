import { useCallback, useEffect, useRef, useState } from 'react';
import * as collectionsApi from '../lib/api/collections';
import type { Collection, ModelFile } from '../types';

export function useCollections() {
  const [collections, setCollections] = useState<Collection[]>([]);
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
      if (!activeCollection) return;
      const updates = orderedIds.map((fileId, position) => ({ fileId, position }));
      setMembers((prev) => {
        const byId = new Map(prev.models.map((m) => [m.id, m]));
        return { ...prev, models: orderedIds.map((id) => byId.get(id)).filter((m): m is ModelFile => m !== undefined) };
      });
      collectionsApi.reorderCollection(activeCollection, updates).catch((e) => {
        console.error('[collections] reordering failed:', e);
      });
    },
    [activeCollection],
  );

  const createCollection = useCallback(
    (name: string) => collectionsApi.createCollection(name).then(() => refreshCollections()),
    [refreshCollections],
  );

  const renameCollection = useCallback(
    (id: string, name: string) => collectionsApi.renameCollection(id, name).then(() => refreshCollections()),
    [refreshCollections],
  );

  const deleteCollection = useCallback(
    (id: string) =>
      collectionsApi.deleteCollection(id).then(() => {
        refreshCollections();
        if (activeCollection === id) setActiveCollection(null);
      }),
    [refreshCollections, activeCollection],
  );

  const addModelToCollection = useCallback(
    (fileId: string, collectionId: string) =>
      collectionsApi.addFilesToCollection(collectionId, [fileId]).then(async () => {
        await Promise.all([
          refreshCollections(),
          ...(activeRef.current === collectionId ? [refreshCollectionModels(collectionId)] : []),
        ]);
      }),
    [refreshCollections, refreshCollectionModels],
  );

  const bulkAddToCollection = useCallback(
    (fileIds: string[], collectionId: string) =>
      collectionsApi.addFilesToCollection(collectionId, fileIds).then(async () => {
        await Promise.all([
          refreshCollections(),
          ...(activeRef.current === collectionId ? [refreshCollectionModels(collectionId)] : []),
        ]);
      }),
    [refreshCollections, refreshCollectionModels],
  );

  const bulkRemoveFromCollection = useCallback(
    (fileIds: string[]) => {
      if (!activeCollection) return Promise.resolve();
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
