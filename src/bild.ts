/// Bildfenster: ein Bild des Archivs, groß.
///
/// Beim Nachschneiden von Buchfiguren steht die Frage immer gleich — sitzt der
/// Ausschnitt richtig? Dafür braucht es das Bild in seiner wahren Größe, nicht
/// als Vorschau in einer Liste, und mehrere nebeneinander. Darum ein Fenster je
/// Bild statt einer Ansicht im Panel.
///
/// Der Grund ist wahlweise die Fläche des Themes oder ein Karomuster: eine
/// freigestellte Figur mit durchsichtigem Grund ist sonst nicht von einer mit
/// weißem zu unterscheiden.

import "./panel-window.css";
import "./bild-window.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { applyTheme, THEMES } from "./themes";
import { applyI18n, t } from "./messages";
import { initZoom } from "./zoom";

applyI18n();
initZoom(document.getElementById("zoom-anker")!, "bild");
const frage = new URLSearchParams(location.search);
const project = frage.get("project")!;
const id = frage.get("id")!;
const win = getCurrentWebviewWindow();

// Dekorationsloses Fenster (Linux): eigene Knöpfe und Resize-Zonen; macOS
// behält die native Ampel.
const isMac = /Mac|Macintosh/.test(navigator.userAgent);
document.documentElement.dataset.platform = isMac ? "mac" : "other";
if (!isMac) {
  void invoke<boolean>("window_buttons_left").then((links) => {
    if (links) document.documentElement.dataset.winbtns = "left";
  });
}
document.getElementById("win-close")!.addEventListener("click", () => win.close());
if (!isMac) {
  document.getElementById("win-min")!.addEventListener("click", () => win.minimize());
  document
    .getElementById("win-max")!
    .addEventListener("click", () => win.toggleMaximize());
  for (const g of document.querySelectorAll<HTMLElement>(".grip")) {
    g.addEventListener("mousedown", (e) => {
      e.preventDefault();
      win.startResizeDragging(g.dataset.dir as never);
    });
  }
}

applyTheme(THEMES[frage.get("theme") || "mocha"]);

const bild = document.getElementById("bild") as HTMLImageElement;
const buehne = document.getElementById("buehne")!;
const heading = document.getElementById("heading")!;
const pfad = document.getElementById("pfad")!;
const masse = document.getElementById("masse")!;
const faktorAnzeige = document.getElementById("faktor")!;

const name = id.replace(/^path:/, "");
heading.textContent = name.split("/").pop() ?? name;
pfad.textContent = name;
void win.setTitle(heading.textContent);

/// Vergrößerung; 0 heißt „einpassen" und wird beim Zeichnen ausgerechnet.
let faktor = 0;

function zeichne() {
  if (!bild.naturalWidth) return;
  if (faktor === 0) {
    // Einpassen: nie über die wahre Größe hinaus — ein 40-Pixel-Zeichen
    // aufgeblasen zeigt Treppen, nicht die Figur.
    const platz = buehne.getBoundingClientRect();
    faktor = Math.min(
      1,
      (platz.width - 32) / bild.naturalWidth,
      (platz.height - 32) / bild.naturalHeight,
    );
  }
  bild.style.width = `${bild.naturalWidth * faktor}px`;
  faktorAnzeige.textContent = `${Math.round(faktor * 100)} %`;
}

function stufe(richtung: number) {
  faktor = Math.min(16, Math.max(0.05, (faktor || 1) * (richtung > 0 ? 1.25 : 0.8)));
  zeichne();
}

document.getElementById("groesser")!.addEventListener("click", () => stufe(1));
document.getElementById("kleiner")!.addEventListener("click", () => stufe(-1));
document.getElementById("passend")!.addEventListener("click", () => {
  faktor = 0;
  zeichne();
});
document.getElementById("karo")!.addEventListener("click", () => {
  buehne.classList.toggle("karo");
});
// Am Rad zoomen, wie in jedem Bildbetrachter.
buehne.addEventListener("wheel", (e) => {
  if (!e.ctrlKey && Math.abs(e.deltaY) < 1) return;
  e.preventDefault();
  stufe(e.deltaY < 0 ? 1 : -1);
});
window.addEventListener("resize", () => {
  if (faktor === 0) zeichne();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") void win.close();
  if (e.key === "+" || e.key === "=") stufe(1);
  if (e.key === "-") stufe(-1);
  if (e.key === "0") {
    faktor = 0;
    zeichne();
  }
});

bild.addEventListener("load", () => {
  masse.textContent = t("image.size", {
    w: String(bild.naturalWidth),
    h: String(bild.naturalHeight),
  });
  zeichne();
});

invoke<string>("archive_image", { project, id }).then(
  (daten) => (bild.src = daten),
  (e) => {
    const fehler = document.createElement("pre");
    fehler.className = "bild-fehler";
    fehler.textContent = String(e);
    buehne.replaceChildren(fehler);
  },
);
