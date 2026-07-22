/// Archiv-Formular: klappt unter dem Archiv-Button auf und fragt Ordner,
/// Beschreibung und Schlagwörter ab (alles optional); Enter oder der Button
/// archivieren, Escape schließt. Gemeinsam für das angedockte Panel und das
/// abgelöste Fenster.

import { t } from "./messages";

export interface ArchiveFormMeta {
  folder?: string;
  description?: string;
  tags: string[];
}

export function initArchiveForm(
  anchor: HTMLElement,
  onSubmit: (meta: ArchiveFormMeta) => void,
): { toggle(): void } {
  function field(placeholder: string): HTMLInputElement {
    const i = document.createElement("input");
    i.type = "text";
    i.placeholder = placeholder;
    return i;
  }

  const form = document.createElement("div");
  form.className = "archive-form";
  form.hidden = true;
  const folder = field(t("archiveForm.folder"));
  const desc = field(t("archiveForm.description"));
  const tags = field(t("archiveForm.tags"));
  const submit = document.createElement("button");
  submit.className = "archive-form-submit";
  submit.textContent = t("archiveForm.submit");
  form.append(folder, desc, tags, submit);
  document.body.append(form);

  const meta = (): ArchiveFormMeta => ({
    folder: folder.value.trim() || undefined,
    description: desc.value.trim() || undefined,
    tags: tags.value
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean),
  });
  // Leeren, weil das Formular zum Fenster gehört und nicht zum Dokument.
  const close = () => {
    form.hidden = true;
    folder.value = "";
    desc.value = "";
    tags.value = "";
  };
  const open = () => {
    const r = anchor.getBoundingClientRect();
    form.style.top = `${r.bottom + 6}px`;
    form.style.right = `${Math.max(8, window.innerWidth - r.right - 4)}px`;
    form.hidden = false;
    folder.focus();
  };
  const fire = () => {
    onSubmit(meta());
    close();
  };
  submit.addEventListener("click", fire);
  form.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      fire();
    } else if (e.key === "Escape") {
      close();
    }
  });

  return { toggle: () => (form.hidden ? open() : close()) };
}
