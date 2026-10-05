import { useEffect, useState } from 'react';
import { getLastPrinterForFile, type LastPrinter } from '../lib/api/lastPrinter';
export function useLastPrinter(fileId: string | null, refreshKey = '') {
  const [result, setResult] = useState<{ fileId: string | null; value: LastPrinter | null }>({ fileId: null, value: null });
  useEffect(() => {
    let current = true;
    setResult({ fileId, value: null });
    if (fileId) getLastPrinterForFile(fileId).then(value => {
      if (current) setResult({ fileId, value });
    }).catch(() => { if (current) setResult({ fileId, value: null }); });
    return () => { current = false; };
  }, [fileId, refreshKey]);
  return result.fileId === fileId ? result.value : null;
}
