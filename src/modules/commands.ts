import { invoke } from "@tauri-apps/api/core";
import { initCommandsView } from "../commands-view";
import type { PanelTab } from "./index";

export const commandsTab: PanelTab = {
  mode: "commands",
  module: "commands",
  buffer: "commands",
  labelKey: "panel.tabCommands",
  titleKey: "panel.tabCommandsTitle",
  init: (container, ctx) =>
    initCommandsView(container, (id) =>
      invoke("commands_delete", { project: ctx.project, id }),
    ),
};
