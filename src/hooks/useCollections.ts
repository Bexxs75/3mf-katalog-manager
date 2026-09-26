import { useCallback, useEffect, useState } from 'react';
import * as collectionsApi from '../lib/api/collections';
import type { Collection, ModelFile } from '../types';

export function useCollections() {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [activeCollection, setActiveCollection] = useState<string | null>(null);
  const [collectionsGalleryOpen, setCollectionsGalleryOpen] = useState(false);
  const [collectionModels, setCollectionModels] = useState<ModelFile[]>([]);

  const refreshCollections = useCallback(() => collectionsApi.listCollections().then(setCollections), []);

  const refreshCollectionModels = useCallback(
    (collectionId: string) => collectionsApi.listCollectionFiles(collectionId).then(setCollectionModels),
    [],
  );

  useEffect(() => {
    refreshCollections();
  }, [refreshCollections]);

  useEffect(() => {
    if (activeCollection) {
      refreshCollectionModels(activeCollection);
    } else {
      setCollectionModels([]);
    }
  }, [activeCollection, refreshCollectionModels]);

  const reorderCollection = useCallback(
    (orderedIds: string[]) => {
      if (!activeCollection) return;
      const updates = orderedIds.map((fileId, position) => ({ fileId, position }));
      setCollectionModels((prev) => {
        const byId = new Map(prev.map((m) => [m.id, m]));
        return orderedIds.map((id) => byId.get(id)).filter((m): m is ModelFile => m !== undefined);
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
      collectionsApi.addFilesToCollection(collectionId, [fileId]).then(() => {
        refreshCollections();
        if (activeCollection === collectionId) refreshCollectionModels(collectionId);
      }),
    [refreshCollections, refreshCollectionModels, activeCollection],
  );

  const bulkAddToCollection = useCallback(
    (fileIds: string[], collectionId: string) =>
      collectionsApi.addFilesToCollection(collectionId, fileIds).then(() => {
        refreshCollections();
        if (activeCollection === collectionId) refreshCollectionModels(collectionId);
      }),
    [refreshCollections, refreshCollectionModels, activeCollection],
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
    refreshCollections,
    reorderCollection,
    createCollection,
    renameCollection,
    deleteCollection,
    addModelToCollection,
    bulkAddToCollection,
    bulkRemoveFromCollection,
  };
}
