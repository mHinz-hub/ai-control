/// Schriftgröße eines Fensters: eine Wippe aus zwei Knöpfen in der Kopfleiste.
///
/// Nicht die Schriftgröße einzelner Regeln, sondern der Maßstab des ganzen
/// Fensters (`setZoom`): die Oberfläche mißt in Pixeln — Knöpfe, Leisten,
/// Abstände —, und eine größere Schrift allein sprengte sie. Der Maßstab
/// vergrößert alles im selben Verhältnis, wie in einem Browser.
///
/// Gemerkt wird je Fensterart, nicht je Fenster: wer das Archiv größer
/// braucht, braucht es beim nächsten Öffnen wieder größer.

import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { t } from "./messages";

const STUFEN = [0.7, 0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.6, 1.8];

/// Baut die Wippe in `leiste` und stellt den gemerkten Maßstab her.
/// `art` ist der Schlüssel im Speicher — eine Fensterart, kein Fenster.
export function initZoom(leiste: HTMLElement, art: string): void {
  const win = getCurrentWebviewWindow();
  const schluessel = `zoom.${art}`;
  const gemerkt = Number(localStorage.getItem(schluessel));
  let i = STUFEN.indexOf(gemerkt);
  if (i < 0) i = STUFEN.indexOf(1);

  const anzeige = document.createElement("span");
  anzeige.className = "zoom-wert";

  function stelle(neu: number) {
    i = Math.min(STUFEN.length - 1, Math.max(0, neu));
    const faktor = STUFEN[i];
    void win.setZoom(faktor);
    localStorage.setItem(schluessel, String(faktor));
    anzeige.textContent = `${Math.round(faktor * 100)} %`;
  }

  const knopf = (zeichen: string, titel: string, schritt: number) => {
    const b = document.createElement("button");
    b.className = "panel-btn zoom-btn";
    b.textContent = zeichen;
    b.title = t(titel);
    b.addEventListener("click", () => stelle(i + schritt));
    return b;
  };

  const wippe = document.createElement("div");
  wippe.className = "zoom-wippe";
  wippe.append(knopf("−", "zoom.smaller", -1), anzeige,
               knopf("+", "zoom.larger", 1));
  // Doppelklick auf die Anzeige stellt den Maßstab zurück — der kürzeste Weg
  // aus einer verstellten Ansicht.
  anzeige.addEventListener("dblclick", () => stelle(STUFEN.indexOf(1)));
  leiste.append(wippe);
  stelle(i);
}
