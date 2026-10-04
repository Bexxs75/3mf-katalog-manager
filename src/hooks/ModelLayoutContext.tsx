import { createContext, useRef, type ReactNode } from 'react';

export interface ModelLayout {
  offsetTop?: number;
  columns: number;
  order: string[];
  scrollToIndex: (index: number) => void;
}
interface LayoutRegistry { layouts: Map<object, ModelLayout> }
export const ModelLayoutContext = createContext<LayoutRegistry | null>(null);
export function ModelLayoutProvider({ children }: { children: ReactNode }) {
  const registry = useRef<LayoutRegistry>({ layouts: new Map() });
  return <ModelLayoutContext.Provider value={registry.current}>{children}</ModelLayoutContext.Provider>;
}
