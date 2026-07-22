/// Übersetzungstabellen für alle Fenster plus ein Vue-freier `t()`-Helfer.
/// Die Vue-Fenster (index.html) ziehen die Tabellen über i18n.ts in vue-i18n;
/// Terminal-, Panel- und Popup-Fenster haben kein Vue und nutzen `t()` und
/// `applyI18n()` direkt — sonst landete vue-i18n samt Vue in ihren Bundles.

export type Locale = "de" | "en";

const de = {
  app: {
    projects: "Projekte",
    pools: "Pools",
    usage: "Verbrauch",
    autostart: "Autostart",
  },
  usage: {
    days7: "7 Tage",
    days30: "30 Tage",
    pool: "Pool",
    project: "Projekt",
    input: "Input",
    output: "Output",
    cacheWrite: "Cache ↑",
    cacheRead: "Cache ↓",
    cost: "≈ Kosten",
    total: "Summe",
    empty: "Keine Verbrauchsdaten im Zeitraum.",
    estimateNote:
      "API-Gegenwert aus den lokalen Transcripts — keine Abrechnung, Historie nur solange claude sie vorhält.",
  },
  projects: {
    project: "Projekt",
    pool: "Pool",
    noPool: "– kein Pool –",
    start: "Starten",
    stop: "Beenden",
    terminalSettings: "Terminal-Einstellungen",
    stillRunning: "{name} läuft noch",
    poolChangeSaved:
      "Der Pool-Wechsel ist gespeichert. Die laufende Session arbeitet aber weiter im alten Pool — der neue gilt erst ab dem nächsten Start.",
    restartHint:
      "Der Neustart beendet die laufende Session. Der automatische commit+push von claude-sync nach Sessionende entfällt dabei.",
    keepRunning: "Weiterlaufen lassen",
    restartNow: "Jetzt neu starten",
    restarting: "Starte neu …",
    terminal: "Terminal — {name}",
    title: "Titel",
    theme: "Theme",
    dockIcon: "Dock-Icon",
    chooseFile: "Datei wählen …",
    remove: "Entfernen",
    defaultIcon: "Standard-Icon",
    appliesNextStart: "Gilt ab dem nächsten Terminal-Start.",
    cancel: "Abbrechen",
    save: "Speichern",
    noPoolAssigned: "kein Pool",
    needsPool: "Kein Pool zugeordnet",
    needsKey: "Pool hat keinen API-Key",
    newProject: "+ Projekt",
    addFolder: "Projekt importieren …",
    addFolderTitle: "Bestehenden Ordner als Projekt aufnehmen",
    addWorkDir: "Ordner hinzufügen …",
    archive: "Archiv",
    archiveNone: "kein Archiv — Panel ohne Wiki und Suche",
    changeDir: "Ändern …",
    wizardTitle: "Neues Projekt",
    name: "Name",
    projectDir: "Projektordner",
    projectDirCustom: "Ordner wählen …",
    workDir: "Arbeitsordner",
    workDirNone: "keiner",
    workDirCustom: "vorhandenen Ordner verknüpfen",
    chooseFolder: "Ordner wählen …",
    create: "Anlegen",
    wizardHint:
      "Angelegt werden Projektordner mit memory/, Sentinel-.gitignore, settings.json (Memory-Verzeichnis + Berechtigungen) und die Pool-/Terminal-Zuordnung.",
    delete: "Projekt löschen",
    deleteRunning: "Läuft noch — erst beenden",
    deleteTitle: "{name} löschen",
    deleteConfirm: "Entfernen",
    deleteWorkDirs: "Arbeitsordner mitlöschen:",
    scopeIntegration: "Nur Integration entfernen",
    scopeIntegrationDesc:
      "ai-control-Spuren werden entfernt; Projektordner, memory/ und Todoliste bleiben.",
    scopeArchive: "Integration & Archiv",
    scopeArchiveDesc: "zusätzlich {path} — endgültig.",
    scopeFull: "Projekt komplett löschen",
    scopeFullDesc: "zusätzlich der Projektordner {path} — endgültig.",
    deletePreviewTitle: "Wird entfernt:",
    docCount: "kein Dokument | 1 Dokument | {count} Dokumente",
    artRegistry: "Registry-Eintrag",
    artAiControl: ".ai-control/ (Config + Icon)",
    artArchivePerm: "Archiv-Berechtigung in .claude/settings.json",
    artTodoHook: "Todo-Hook in .claude/settings.json",
    artPanelFiles: "Panel-Kanaldatei | {count} Panel-Kanaldateien",
    artDesktop: ".desktop-Starter",
    artArchive: "Archiv {path}",
    artProjectDir: "Projektordner {path}",
    artWorkDir: "Arbeitsordner {path}",
    todo: "Todoliste",
    todoDesc: "OFFENE-PUNKTE.md bei jedem Sessionstart einspielen",
    archiveChangeTitle: "Archiv wechseln",
    archiveChangeText: "Das Archiv wechselt von {old} nach {neu}.",
    archiveMigrate: "Dokumente ins neue Archiv verschieben",
    archiveMigrateHint:
      "Ohne Haken bleibt das bisherige Archiv unverändert liegen; die App verweist künftig nur noch auf das neue.",
    archiveChangeConfirm: "Wechseln",
    groupAppearance: "Darstellung",
    groupFolders: "Ordner",
    groupSession: "Session",
    writesImmediately: "Änderungen hier schreiben sofort — ohne Speichern.",
  },
  pools: {
    pool: "Pool",
    type: "Typ",
    actions: "Aktionen",
    newOauth: "+ oAuth",
    newApikey: "+ apiKey",
    relogin: "Zurücksetzen",
    rename: "Umbenennen",
    renameTitle: "{name} umbenennen",
    changeKey: "Key ändern",
    insertKey: "Key eintragen",
    delete: "Löschen",
    assigned: "kein Projekt | 1 Projekt | {count} Projekte",
    empty: "Noch keine Pools angelegt.",
    newOauthPool: "Neuen oAuth-Pool anlegen",
    newApikeyPool: "Neuen API-Key-Pool anlegen",
    editKey: "API-Key ändern – {name}",
    name: "Name",
    apiKey: "API-Key",
    oauthHint:
      "Der Pool wird angelegt; die Anmeldung macht claude beim ersten Start selbst per /login (Browser gegen dein Abo).",
    newReference: "+ vorhandene Anmeldung",
    newReferencePool: "Vorhandene Anmeldung übernehmen",
    referenceDefaultName: "System",
    referenceHint:
      "Der Pool verweist auf {dir} — claudes eigenes Verzeichnis. Die dortige Anmeldung gilt weiter, ein /login entfällt. Die App legt dort nur den Panel-Zugang an und löscht nichts.",
    referenceLogin: "Anmeldung von claude",
    cancel: "Abbrechen",
    createPool: "Anlegen",
    reset: "Zurücksetzen",
    save: "Speichern",
    deletePool: "Pool löschen",
    deleteWarning:
      "Pool {name} und seine Credential-Datei werden gelöscht. Das lässt sich nicht rückgängig machen.",
    deleteUnassigns: "Diese Projekte verlieren ihre Pool-Zuordnung:",
    deleteBlockedTooltip:
      "Löschen gesperrt — {projects} läuft gerade. Erst beenden.",
    reloginTitle: "{name} zurücksetzen",
    reloginWarning:
      "Der im Schlüsselbund gespeicherte Zugriffstoken von {name} wird gelöscht. Beim nächsten Start verlangt claude erneut /login.",
    reloginNoEntry:
      "Kein Schlüsselbund-Eintrag vorhanden — beim nächsten Start meldet sich claude ohnehin per /login an.",
    reloginBlocked:
      "Zurücksetzen ist nur bei ungenutztem Pool möglich. Diese Projekte laufen noch:",
    keychainUnavailableTitle: "Keine sichere Ablage",
    keychainUnavailable:
      "Keychain/Keyring ist nicht verfügbar. Der Key kann stattdessen ungesichert als Datei im Pool-Ordner liegen (0600).",
    storeAsFile: "Als Datei ablegen",
  },
  panel: {
    windowTerminal: "Terminal",
    windowDocument: "Dokument",
    windowProjects: "Projekte",
    tabCommands: "Befehle",
    tabCommandsTitle: "Befehls-History",
    tabDraft: "Dokument",
    tabDraftTitle: "Dokument",
    tabWiki: "Wiki",
    tabWikiTitle: "Archiv-Wiki",
    tabSearch: "Suche",
    tabSearchTitle: "Suchtreffer",
    minimize: "Minimieren",
    maximize: "Maximieren",
    close: "Schließen",
    closePanel: "Panel schließen",
    hidePanel: "Panel ausblenden",
    detach: "In eigenes Fenster ablösen",
    dock: "Wieder andocken",
    editTitle: "Titel bearbeiten",
    spellcheckLang: "Sprache der Rechtschreibprüfung",
    openInWiki: "Im Wiki öffnen",
    toggleRaw: "Rohtext / gerendert",
    rendered: "MD",
    raw: "Roh",
    editDraft: "Entwurf bearbeiten (Cmd/Ctrl+Enter speichert, Esc verwirft)",
    copy: "In die Zwischenablage kopieren",
    archive: "In den Archiv-Ordner speichern",
    chooseArchiveDir: "Archiv-Ordner wählen",
    processEnded: "[Prozess beendet]",
  },
  popup: {
    open: "Öffnen",
    quit: "Beenden",
  },
  commands: {
    copyOne: "Befehl kopieren",
    removeOne: "Aus der History entfernen",
    copyAll: "Alle kopieren",
    session: "Session",
  },
  archiveForm: {
    folder: "Ordner — z. B. konzepte/panel",
    description: "Beschreibung",
    tags: "Schlagwörter, kommagetrennt",
    submit: "Archivieren",
  },
  search: {
    placeholder: "Archiv durchsuchen — #tag filtert",
    minChars: "Mindestens 3 Zeichen — oder Enter.",
    hits: "{count} Treffer {scope}",
    noHits: "Keine Treffer für {scope}",
  },
  wiki: {
    archive: "Archiv",
    docOne: "{count} Dokument",
    docMany: "{count} Dokumente",
    all: "Alle",
    backlinks: "Verweise hierher",
    backlinksLabel: "Verweise hierher: ",
    emptyTag: "Keine Dokumente mit #{tag}.",
    emptyArchive: "Das Archiv ist leer.",
    emptyHint:
      "Archivieren: Archiv-Button im Entwurf oder „archiviere das“ im Chat — mit Ordner, Beschreibung und Schlagwörtern.",
    recent: "Zuletzt",
    root: "Wurzel",
    back: "‹ Archiv",
    noPage: "Keine Wiki-Seite geladen.",
    openOverview: "Archiv-Übersicht öffnen",
  },
};

const en: typeof de = {
  app: {
    projects: "Projects",
    pools: "Pools",
    usage: "Usage",
    autostart: "Autostart",
  },
  usage: {
    days7: "7 days",
    days30: "30 days",
    pool: "Pool",
    project: "Project",
    input: "Input",
    output: "Output",
    cacheWrite: "Cache ↑",
    cacheRead: "Cache ↓",
    cost: "≈ cost",
    total: "Total",
    empty: "No usage data in this period.",
    estimateNote:
      "API equivalent from local transcripts — not a bill; history only as long as claude keeps it.",
  },
  projects: {
    project: "Project",
    pool: "Pool",
    noPool: "– no pool –",
    start: "Start",
    stop: "Stop",
    terminalSettings: "Terminal settings",
    stillRunning: "{name} is still running",
    poolChangeSaved:
      "The pool change is saved. The running session keeps working in the old pool — the new one applies from the next start.",
    restartHint:
      "Restarting ends the running session. The automatic commit+push by claude-sync after the session is skipped.",
    keepRunning: "Keep running",
    restartNow: "Restart now",
    restarting: "Restarting …",
    terminal: "Terminal — {name}",
    title: "Title",
    theme: "Theme",
    dockIcon: "Dock icon",
    chooseFile: "Choose file …",
    remove: "Remove",
    defaultIcon: "Default icon",
    appliesNextStart: "Applies from the next terminal start.",
    cancel: "Cancel",
    save: "Save",
    noPoolAssigned: "no pool",
    needsPool: "No pool assigned",
    needsKey: "Pool has no API key",
    newProject: "+ Project",
    addFolder: "Import project …",
    addFolderTitle: "Add an existing folder as a project",
    addWorkDir: "Add folder …",
    archive: "Archive",
    archiveNone: "no archive — panel without wiki and search",
    changeDir: "Change …",
    wizardTitle: "New project",
    name: "Name",
    projectDir: "Project folder",
    projectDirCustom: "Choose folder …",
    workDir: "Working folder",
    workDirNone: "none",
    workDirCustom: "link existing folder",
    chooseFolder: "Choose folder …",
    create: "Create",
    wizardHint:
      "Creates the project folder with memory/, sentinel .gitignore, settings.json (memory dir + permissions), and the pool/terminal assignment.",
    delete: "Delete project",
    deleteRunning: "Still running — stop it first",
    deleteTitle: "Delete {name}",
    deleteConfirm: "Remove",
    deleteWorkDirs: "Also delete working folders:",
    scopeIntegration: "Remove integration only",
    scopeIntegrationDesc:
      "ai-control traces are removed; project folder, memory/ and todo list stay.",
    scopeArchive: "Integration & archive",
    scopeArchiveDesc: "additionally {path} — permanent.",
    scopeFull: "Delete project completely",
    scopeFullDesc: "additionally the project folder {path} — permanent.",
    deletePreviewTitle: "Will be removed:",
    docCount: "no documents | 1 document | {count} documents",
    artRegistry: "registry entry",
    artAiControl: ".ai-control/ (config + icon)",
    artArchivePerm: "archive permission in .claude/settings.json",
    artTodoHook: "todo hook in .claude/settings.json",
    artPanelFiles: "panel channel file | {count} panel channel files",
    artDesktop: ".desktop launcher",
    artArchive: "archive {path}",
    artProjectDir: "project folder {path}",
    artWorkDir: "working folder {path}",
    todo: "Todo list",
    todoDesc: "Inject OFFENE-PUNKTE.md at every session start",
    archiveChangeTitle: "Change archive",
    archiveChangeText: "The archive changes from {old} to {neu}.",
    archiveMigrate: "Move documents to the new archive",
    archiveMigrateHint:
      "Unchecked, the previous archive stays untouched; the app just points to the new one.",
    archiveChangeConfirm: "Change",
    groupAppearance: "Appearance",
    groupFolders: "Folders",
    groupSession: "Session",
    writesImmediately: "Changes here are written immediately — no save step.",
  },
  pools: {
    pool: "Pool",
    type: "Type",
    actions: "Actions",
    newOauth: "+ oAuth",
    newApikey: "+ apiKey",
    relogin: "Reset",
    rename: "Rename",
    renameTitle: "Rename {name}",
    changeKey: "Change key",
    insertKey: "Insert key",
    delete: "Delete",
    assigned: "no projects | 1 project | {count} projects",
    empty: "No pools yet.",
    newOauthPool: "Create oAuth pool",
    newApikeyPool: "Create API key pool",
    editKey: "Change API key – {name}",
    name: "Name",
    apiKey: "API key",
    oauthHint:
      "The pool is created; claude signs in itself on first start via /login (browser against your subscription).",
    newReference: "+ existing login",
    newReferencePool: "Adopt existing login",
    referenceDefaultName: "System",
    referenceHint:
      "The pool points at {dir} — claude's own directory. The login there keeps working, so no /login is needed. The app only adds panel access there and deletes nothing.",
    referenceLogin: "login from claude",
    cancel: "Cancel",
    createPool: "Create",
    reset: "Reset",
    save: "Save",
    deletePool: "Delete pool",
    deleteWarning:
      "Pool {name} and its credential file will be deleted. This cannot be undone.",
    deleteUnassigns: "These projects lose their pool assignment:",
    deleteBlockedTooltip:
      "Deletion blocked — active: {projects}. Stop the session first.",
    reloginTitle: "Reset {name}",
    reloginWarning:
      "The access token for {name} stored in the keychain will be deleted. On the next start claude asks for /login again.",
    reloginNoEntry:
      "No keychain entry present — claude signs in via /login on the next start anyway.",
    reloginBlocked:
      "Resetting requires an unused pool. These projects are still running:",
    keychainUnavailableTitle: "No secure storage",
    keychainUnavailable:
      "Keychain/keyring is unavailable. The key can instead be stored unprotected as a file in the pool folder (0600).",
    storeAsFile: "Store as file",
  },
  panel: {
    windowTerminal: "Terminal",
    windowDocument: "Document",
    windowProjects: "Projects",
    tabCommands: "Commands",
    tabCommandsTitle: "Command history",
    tabDraft: "Document",
    tabDraftTitle: "Document",
    tabWiki: "Wiki",
    tabWikiTitle: "Archive wiki",
    tabSearch: "Search",
    tabSearchTitle: "Search hits",
    minimize: "Minimize",
    maximize: "Maximize",
    close: "Close",
    closePanel: "Close panel",
    hidePanel: "Hide panel",
    detach: "Detach into its own window",
    dock: "Dock again",
    editTitle: "Edit title",
    spellcheckLang: "Spell-check language",
    openInWiki: "Open in wiki",
    toggleRaw: "Raw text / rendered",
    rendered: "MD",
    raw: "Raw",
    editDraft: "Edit draft (Cmd/Ctrl+Enter saves, Esc discards)",
    copy: "Copy to clipboard",
    archive: "Save to the archive folder",
    chooseArchiveDir: "Choose archive folder",
    processEnded: "[process ended]",
  },
  popup: {
    open: "Open",
    quit: "Quit",
  },
  commands: {
    copyOne: "Copy command",
    removeOne: "Remove from history",
    copyAll: "Copy all",
    session: "Session",
  },
  archiveForm: {
    folder: "Folder — e.g. concepts/panel",
    description: "Description",
    tags: "Tags, comma-separated",
    submit: "Archive",
  },
  search: {
    placeholder: "Search the archive — #tag filters",
    minChars: "At least 3 characters — or press Enter.",
    hits: "{count} hits {scope}",
    noHits: "No hits for {scope}",
  },
  wiki: {
    archive: "Archive",
    docOne: "{count} document",
    docMany: "{count} documents",
    all: "All",
    backlinks: "Links here",
    backlinksLabel: "Links here: ",
    emptyTag: "No documents tagged #{tag}.",
    emptyArchive: "The archive is empty.",
    emptyHint:
      "To archive: the archive button in the draft, or “archive this” in the chat — with folder, description and tags.",
    recent: "Recent",
    root: "Root",
    back: "‹ Archive",
    noPage: "No wiki page loaded.",
    openOverview: "Open archive overview",
  },
};

export const messages = { de, en };

/// Gewählte Sprache: der Schalter aus dem Hauptfenster (localStorage), sonst
/// die Browser-Sprache. Beide Wege teilen sich den Schlüssel `lang` mit
/// setLocale() in i18n.ts.
export function storedLocale(): Locale {
  // Über window und in try/catch: gesperrter Storage (privater Modus) wirft
  // schon beim Lesen, und unter Node schattet ein undefiniertes Global das
  // localStorage der Testumgebung.
  let stored: string | null = null;
  try {
    stored = window.localStorage.getItem("lang");
  } catch {
    stored = null;
  }
  if (stored === "de" || stored === "en") return stored;
  return navigator.language.startsWith("de") ? "de" : "en";
}

/// Übersetzt einen Punkt-Pfad (`panel.tabDraft`) und setzt `{platzhalter}` ein.
/// Fehlt der Schlüssel, kommt der Pfad selbst zurück — im UI sichtbar statt
/// still leer.
export function t(key: string, params?: Record<string, string | number>): string {
  const table: unknown = messages[storedLocale()];
  const hit = key
    .split(".")
    .reduce<unknown>((o, part) => (o as Record<string, unknown>)?.[part], table);
  if (typeof hit !== "string") return key;
  return params
    ? hit.replace(/\{(\w+)\}/g, (m, name) => String(params[name] ?? m))
    : hit;
}

/// Beschriftet statisches Markup: `data-i18n` setzt den Textinhalt (auch am
/// <title> im Kopf), `data-i18n-title`/`-aria`/`-placeholder` das jeweilige
/// Attribut. Einmal beim Fensterstart aufgerufen — die Sprache wechselt erst
/// mit dem nächsten Start, es gibt in diesen Fenstern keinen Umschalter.
export function applyI18n(root: ParentNode = document) {
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n]")) {
    el.textContent = t(el.dataset.i18n!);
  }
  const attrs: [string, string][] = [
    ["data-i18n-title", "title"],
    ["data-i18n-aria", "aria-label"],
    ["data-i18n-placeholder", "placeholder"],
  ];
  for (const [marker, attr] of attrs) {
    for (const el of root.querySelectorAll<HTMLElement>(`[${marker}]`)) {
      el.setAttribute(attr, t(el.getAttribute(marker)!));
    }
  }
  document.documentElement.lang = storedLocale();
}
