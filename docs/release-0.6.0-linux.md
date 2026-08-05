# Übergabe an Linux — aiCentral 0.6.0

## Stand auf dem Mac

`origin/main` = `a192b41`. Darauf aufgesetzt zwei Änderungen, die noch im
Arbeitsbaum liegen: `src/commands-view.ts` (Breiten der Tab-Leiste) und
`src/test-setup.ts` (`document.fonts`-Stub für happy-dom).

Gebaut und installiert: `~/Applications/ai-central.app`,
`ai-central_0.6.0_aarch64.dmg`. Das DMG liegt auf der 2TB-Platte.

## Was seit `c1dfbdb` dazugekommen ist

Aus der Durchsicht der zwei Panel-Fenster kamen zwei Befunde, beide gefixt:

- **Das Sitzungs-Panel kam nicht ins Dock zurück.** `panel.ts` blendete den
  Andock-Knopf hart aus — ein Rest aus der Zeit, als die Flächen fest verteilt
  waren. Der Knopf steht jetzt im Sitzungs-Fenster (im Archiv-Fenster nicht,
  das hat kein Dock), schickt beim Klick `panel-attached` mit dem zuletzt
  offenen Tab und schließt das Fenster; `terminal.ts` holt das Panel zurück
  und schaltet auf genau diesen Tab.
- **Ein Entwurf blieb unsichtbar.** Der `panel-update`-Handler schaltete nur
  auf den Entwurf, wenn der Entwurf oder nichts offen war — bei offener
  ToDo-Liste (nach dem Start der Normalfall) landete der Text in der
  verborgenen Ansicht. Auf der Sitzungsfläche gewinnt jetzt der Entwurf; im
  Archiv-Fenster bleibt der Leser stehen, dort reißt kein Entwurf den Baum weg.

Dazu die Tab-Leiste der Sitzungsfläche:

- **Entwurf ist ein Reiter** neben ToDo und Befehle, sobald es einen Entwurf
  gibt — sonst führte der Weg zu den ToDos vom Entwurf weg, ohne zurück.
- **Kurzformen:** der aktive Tab schreibt sich aus, die beiden anderen stehen
  als Buchstabe da und nennen ihren Namen im Hover — DE T · B · E,
  EN T · C · D.
- **Stehende Leiste:** beim Laden wird das längste der drei Wörter gemessen;
  der aktive Tab trägt diese Breite, die inaktiven 2em. Damit ist die Leiste in
  jeder Auswahl gleich breit und nichts links davon rückt mehr.
- **Trenner** hinter „Datei": Schriftgröße │ Datei │ Entwurf · ToDo · Befehle
  │ Archiv · Suche.

Tests: 138 Frontend grün, Typecheck sauber. Die vier Punkte aus
`release-0.6.0.md` sind auf dem Mac durchgesehen.

## Auf der Linux-Box

```
cd ~/projects/ai-control
git pull origin main
npm install
npm run tauri build      # deb + rpm
```

Zu prüfen, weil die Änderungen an der Kopfleiste und am Dock hängen:

1. Sitzungs-Panel ablösen, im Fenster den Tab wechseln, andocken — das Dock
   kommt mit demselben Tab zurück.
2. Text ins Panel schreiben, während ToDos offen stehen: der Entwurf zeigt
   sich, der Reiter „Entwurf" erscheint.
3. Tab-Leiste: Auswahl wechseln, die Leiste darf nicht springen.
4. Fensterknöpfe links (GNOME/KDE-Einstellung): die Leiste sitzt richtig.

## Danach, auf Ansage

- Squash-Publish auf github + Tag `v0.6.0`
- GitHub-Release mit `ai-central_0.6.0_amd64.deb`,
  `ai-central-0.6.0-1.x86_64.rpm` und dem DMG von der 2TB-Platte
