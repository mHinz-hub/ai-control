import { createI18n } from "vue-i18n";

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
    deleteWarning:
      "Der Projektordner (inkl. memory/, Settings und Pool-Zuordnung) wird endgültig gelöscht.",
    deleteWorkDirs: "Arbeitsordner mitlöschen:",
    workDirsStay: "Die Arbeitsordner bleiben unangetastet.",
    unlink: "Nur aus Liste nehmen",
    unlinkHint:
      "Nur aus Liste nehmen entfernt den Eintrag; der Ordner bleibt unangetastet.",
    todo: "Todoliste",
    todoDesc: "OFFENE-PUNKTE.md bei jedem Sessionstart einspielen",
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
    deleteWarning:
      "The project folder (incl. memory/, settings and pool assignment) will be deleted permanently.",
    deleteWorkDirs: "Also delete working folders:",
    workDirsStay: "The working folders stay untouched.",
    unlink: "Remove from list only",
    unlinkHint:
      "Remove from list only drops the entry; the folder stays untouched.",
    todo: "Todo list",
    todoDesc: "Inject OFFENE-PUNKTE.md at every session start",
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
};

const stored = localStorage.getItem("lang");
const initial =
  stored === "de" || stored === "en"
    ? stored
    : navigator.language.startsWith("de")
      ? "de"
      : "en";

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: initial,
  fallbackLocale: "en",
  messages: { de, en },
});

export function setLocale(lang: "de" | "en") {
  i18n.global.locale.value = lang;
  localStorage.setItem("lang", lang);
}
