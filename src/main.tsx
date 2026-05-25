import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ReaderStandalone } from "./ReaderStandalone";
import "./styles.css";

function parseReaderId(): number | null {
  const hash = window.location.hash.replace(/^#/, "");
  const params = new URLSearchParams(hash);
  const raw = params.get("reader");
  if (!raw) return null;
  const id = Number(raw);
  return Number.isFinite(id) && id > 0 ? id : null;
}

const readerId = parseReaderId();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {readerId !== null ? <ReaderStandalone bookId={readerId} /> : <App />}
  </React.StrictMode>,
);
