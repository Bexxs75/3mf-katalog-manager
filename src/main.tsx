import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LanguageProvider } from "./i18n/LanguageContext";
import { UiDensityProvider } from "./hooks/UiDensityContext";
import { DiagnosticsProvider } from "./diagnostics/DiagnosticsContext";
import { ErrorBoundary, CrashFallback } from "./diagnostics/ErrorBoundary";
import "./styles/theme.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <UiDensityProvider>
        <DiagnosticsProvider>
          <ErrorBoundary fallback={(reload) => <CrashFallback onReload={reload} />}>
            <App />
          </ErrorBoundary>
        </DiagnosticsProvider>
      </UiDensityProvider>
    </LanguageProvider>
  </React.StrictMode>,
);
