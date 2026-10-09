import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App, createAppDeps } from "./app";
import "./styles/app.css";

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing from index.html");

createRoot(root).render(
  <StrictMode>
    <App deps={createAppDeps()} />
  </StrictMode>,
);
