---
name: panel
description: Beim Entwerfen längerer Texte (ADR, E-Mail, Dokument, Spezifikation, Commit-Message, Textbaustein) das MCP-Tool write_panel nutzen, statt den Text als Fließtext in den Chat zu schreiben; für eine bestehende Datei write_panel mit path aufrufen. Befehle, die der Nutzer ausführen soll, IMMER über write_commands als kopierbare Kacheln ausgeben statt als Codeblock im Chat. Will der Nutzer die Befehlsliste sehen ("zeig die Befehle", "zeige die Befehlsliste"), show_commands aufrufen. Auch auf ausdrückliche Bitte nutzen ("ins Panel", "ins Panel schreiben").
---

# Panel

Claude Code läuft im TUI; sichtbarer Text lässt sich dort nicht sauber selektieren und kopieren. Die ai-control-App zeigt Inhalte in einem andockbaren Panel neben dem Terminal an — selektierbar, mit Copy-Buttons. Die Kanäle dahin sind die MCP-Tools `write_panel` und `write_commands` (Server `text-panel`).

## Entwürfe — `write_panel`

- Automatisch, sobald ein längerer Fließtext oder ein Dokument entworfen wird (ADR, E-Mail, Spezifikation, Commit-Message, Textbaustein); außerdem auf ausdrückliche Bitte, etwas ins Panel zu legen.
- Neuer Text: vollständigen Entwurf als Argument `text` (Markdown-Rohtext, das Panel rendert ihn).
- Bestehende Datei: **immer** `path` statt `text` übergeben — der Server liest die Datei selbst von der Platte, der Inhalt wird nicht abgetippt.
- Den Text nicht zusätzlich als Fließtext in den Chat schreiben — im Chat nur kurz bestätigen (z. B. „Entwurf im Panel.").
- Nicht für normale Chat-Antworten, kurze Bestätigungen oder Code im laufenden Dialog.

## Befehle — `write_commands`

- IMMER nutzen, wenn ein oder mehrere Shell-Befehle für den Nutzer bestimmt sind (er soll sie ausführen) — statt sie als Codeblock in den Chat zu schreiben.
- Argument `commands`: Array in Ausführungsreihenfolge, je Eintrag `cmd` (exakt ausführbar) und optional `note` (Kurznotiz).
- Das Panel zeigt sie als Kacheln mit Copy-Button, angehängt an die Befehls-History der Session (flüchtig, startet mit jeder Session leer); im Chat nur kurz einordnen, den Befehl nicht doppelt ausgeben.
- Nicht für Befehle, die Claude selbst ausführt, oder für Code, der nur erklärt wird.

## Befehlsliste zeigen — `show_commands`

- Nutzen, wenn der Nutzer die bisherige Befehlsliste sehen will („zeig die Befehle", „zeige die Befehlsliste"): schaltet das Panel auf die Kachel-Ansicht der History, ohne etwas anzuhängen. Keine Argumente.

## Panel leer öffnen

Soll das Panel ohne konkreten Inhalt geöffnet werden („mach das Panel auf", „Panel öffnen"), `write_panel` mit dem neutralen Platzhalter `Panel` aufrufen — leerer Text blendet das Panel nicht ein. Keinen erklärenden, werbenden oder ausgedachten Einleitungstext erfinden.

## Fallback

Sind die Tools nicht verfügbar (Terminal außerhalb von ai-control), Text bzw. Befehle normal im Chat ausgeben.
