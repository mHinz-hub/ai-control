import { invoke } from "@tauri-apps/api/core";
import { initTodoView, type TodoEdit } from "../todo-view";
import type { PanelTab } from "./index";

export const todoTab: PanelTab = {
  mode: "todo",
  module: "todo",
  buffer: "todos",
  labelKey: "panel.tabTodo",
  kurzKey: "panel.tabTodoKurz",
  titleKey: "panel.tabTodoTitle",
  init: (container, ctx) =>
    initTodoView(
      container,
      (id) => invoke("todos_delete", { project: ctx.project, id }),
      (todo: TodoEdit) => {
        const args = {
          project: ctx.project,
          text: todo.text,
          note: todo.note ?? null,
          due: todo.due ?? null,
        };
        void (
          todo.id
            ? invoke("todos_update", { ...args, id: todo.id })
            : invoke("todos_add", args)
        ).catch((e) => ctx.toast(String(e)));
      },
    ),
};
