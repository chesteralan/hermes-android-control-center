import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { monospaceFont } from "./lib/platform";
import "./index.css";

document.documentElement.style.setProperty("--font-mono", monospaceFont);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
