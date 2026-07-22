// KWin-Script (Plasma 6): das Pendant zur GNOME-Shell-Extension.
// Unter Wayland darf nur der Compositor ein Fenster positionieren — die App
// (Client) kann das nicht. Dieses Script schiebt unser rahmenloses Popup an
// den Mauszeiger, der beim Tray-Klick auf dem Icon sitzt. Sonst nichts.
//
// Kein Drift: Fenster, UI und Verhalten kommen unverändert aus der App; hier
// wird ausschließlich platziert.

const WIN_TITLE = "ai-control-popup";

function place(w) {
  if (!w || w.caption !== WIN_TITLE) {
    return;
  }
  const cursor = workspace.cursorPos;
  // MaximizeArea = Arbeitsfläche ohne Panels; so sitzt das Popup neben der
  // Leiste, nicht darunter.
  const area = workspace.clientArea(KWin.MaximizeArea, w);
  const width = w.frameGeometry.width;
  const height = w.frameGeometry.height;

  // Rechte Fensterkante an den Zeiger; oben/unten aus dem Klemmen an die
  // Arbeitsflächenkante (Panel oben -> unter dem Icon, Panel unten -> darüber).
  let x = cursor.x - width;
  let y = cursor.y;
  if (x < area.x) x = area.x;
  if (x + width > area.x + area.width) x = area.x + area.width - width;
  if (y < area.y) y = area.y;
  if (y + height > area.y + area.height) y = area.y + area.height - height;

  // Reine QJSEngine, kein QML: `Qt.rect` gibt es hier nicht; die Engine
  // konvertiert ein {x,y,width,height}-Objekt selbst nach QRectF.
  w.frameGeometry = { x: x, y: y, width: width, height: height };
}

// windowAdded feuert beim (erneuten) Mappen des Popups, windowActivated fängt
// das Wiederanzeigen ab, falls der Compositor das Fenster nur reaktiviert.
workspace.windowAdded.connect(place);
workspace.windowActivated.connect(place);
