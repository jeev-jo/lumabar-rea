import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Preview from "./Preview";

// One bundle serves every window; the URL says which one this is.
const view = new URLSearchParams(window.location.search).get("view");
const Root = view === "preview" ? Preview : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
