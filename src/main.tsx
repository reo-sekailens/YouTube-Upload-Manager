import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { CrashBoundary } from "./components/CrashRecoveryScreen";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <CrashBoundary>
      <App />
    </CrashBoundary>
  </StrictMode>,
);
