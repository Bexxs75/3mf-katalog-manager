import { RuntimeEnvironmentProvider } from './hooks/useRuntimeEnvironment';
import { ModelLayoutProvider } from "./hooks/ModelLayoutContext";
import React, { useEffect } from "react";
import { showAfterPaint } from "./lib/startup";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LanguageProvider } from "./i18n/LanguageContext";
import { UiDensityProvider } from "./hooks/UiDensityContext";
import { DiagnosticsProvider } from "./diagnostics/DiagnosticsContext";
import { ErrorBoundary, CrashFallback } from "./diagnostics/ErrorBoundary";
import { installContextMenuGuard } from "./lib/contextMenuGuard";
import "./styles/theme.css";

installContextMenuGuard(document, import.meta.env.DEV);

function StartupReady() {
  useEffect(showAfterPaint, []);
  return null;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RuntimeEnvironmentProvider>
      <StartupReady />
      <LanguageProvider>
        <UiDensityProvider>
          <DiagnosticsProvider>
            <ErrorBoundary fallback={(reload) => <CrashFallback onReload={reload} />}>
              <ModelLayoutProvider><App /></ModelLayoutProvider>
            </ErrorBoundary>
          </DiagnosticsProvider>
        </UiDensityProvider>
      </LanguageProvider>
    </RuntimeEnvironmentProvider>
  </React.StrictMode>,
);
