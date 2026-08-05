# Übergabe an den Mac — aiCentral 0.6.0

## Stand auf lintus

`origin/main` = `0e3b393`. Zwei Commits seit dem letzten Mac-Stand:

- `f87536e` Panel als zwei Fenster (Archiv, Sitzung), Schriftgrößen-Wippe in allen Fenstern, Version 0.6.0
- `0e3b393` Kachelschrift folgt der Terminalschrift; README auf den Stand von 0.6.0

Version steht in allen vier Dateien auf **0.6.0**: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`.

## Was neu ist

- **Zwei Panel-Flächen.** Archiv (Wiki, Suche) läuft nur noch als eigenes Fenster, Sitzung (ToDo, Befehle, Entwurf) ist angedockt und über den Knopf in der Titelzeile ablösbar. Fensterlabel `panel-<flaeche>-<project>`.
- **Schriftgrößen-Wippe** in Terminal-, Panel-, Commit- und Bildfenster (`src/zoom.ts`, neun Stufen 0,7–1,8, gemerkt je Fensterart). Recht `core:webview:allow-set-webview-zoom` in allen vier Capabilities.
- **Leistenordnung** im Hauptfenster: Schriftgröße │ Datei │ ToDo · Befehle │ Archiv · Suche; im Panel-Fenster Schriftgröße │ Tabs.
- **Kachelschrift** folgt angedockt der Terminalschrift (`--tile-font`), abgelöst 12 px plus Wippe.
- **Ablösen und Schließen** stehen als Paar rechts in der Panel-Titelzeile.
- Bildfenster aus `8952fde` (Vorschau in der Liste, eigenes Fenster je Bild) war schon vor diesen beiden Commits drin.

## Auf dem Mac

```
cd ~/projects/ai-control
git pull origin main
npm install          # falls package.json sich geändert hat
./build.sh
```

Zu testen, weil die Änderungen ans Fenster-Handling gehen:

1. Sitzungs-Panel ab- und wieder andocken; Kachelschrift muß angedockt der Terminalschrift folgen (Cmd +/− im Terminal).
2. Archiv über den Wiki-Tab öffnen — eigenes Fenster, nicht andockbar.
3. Wippe in allen vier Fenstern; auf macOS liegt die Ampel links, die Leiste darf nicht verrutschen.
4. Bildfenster aus der Archivliste.

## Danach, auf Ansage

- Squash-Publish auf github + Tag `v0.6.0`
- GitHub-Release mit deb, rpm (auf der Linux-Box gebaut: `ai-central_0.6.0_amd64.deb`, `ai-central-0.6.0-1.x86_64.rpm`) und dem DMG vom Mac
