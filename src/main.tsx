import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LanguageProvider } from "./i18n/LanguageContext";
import { UiDensityProvider } from "./hooks/UiDensityContext";
import "./styles/theme.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <UiDensityProvider>
        <App />
      </UiDensityProvider>
    </LanguageProvider>
  </React.StrictMode>,
);
