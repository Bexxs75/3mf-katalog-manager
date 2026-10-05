import { useEffect, useRef, useState } from 'react';
import { toAppError, type AppError } from '../lib/errors';

/** Keep upload feedback with the model that started the file dialog. */
export function useImageUpload(modelId: string | undefined, upload: () => void | Promise<void>) {
  const [error, setError] = useState<AppError | null>(null);
  const currentModel = useRef(modelId);
  currentModel.current = modelId;
  const request = useRef(0);
  useEffect(() => {
    request.current += 1;
    setError(null);
  }, [modelId]);
  const dismiss = () => setError(null);
  const uploadImage = async () => {
    const id = modelId;
    const token = ++request.current;
    setError(null);
    try {
      await upload();
    } catch (e) {
      if (currentModel.current === id && request.current === token) setError(toAppError(e));
    }
  };
  return { error, dismiss, uploadImage };
}
