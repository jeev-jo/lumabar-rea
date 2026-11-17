import React, { type ComponentType } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Preview from "./Preview";
import SettingsPage from "./SettingsPage";
import Popover from "./Popover";
import WidgetPicker from "./WidgetPicker";

// One bundle serves every window; the URL says which one this is.
const VIEWS: Record<string, ComponentType> = {
  preview: Preview,
  settings: SettingsPage,
  popover: Popover,
  widgets: WidgetPicker,
};
const view = new URLSearchParams(window.location.search).get("view") ?? "";
const Root = VIEWS[view] ?? App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
