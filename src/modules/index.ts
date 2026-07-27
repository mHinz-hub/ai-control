/// Modul-Registry des Panels: welche Tabs es gibt, zu welchem Backend-Modul
/// sie gehören und wie ihre Ansicht entsteht. panel-wiring baut Tab-Knöpfe
/// und Content-Container aus dieser Liste; welche Tabs erscheinen,
/// entscheidet `enabled_modules` (Backend-Registry in domain/modules.rs).
/// Puffer-Konvention: Erstbefüllung per `buffer_read`, Updates kommen als
/// `<buffer>-update`-Event.

import { commandsTab } from "./commands";
import { searchTab, wikiTab } from "./archive";
import { todoTab } from "./todo";

export interface ModuleView {
  set(text: string): void;
  empty(): boolean;
}

/// Gemeinsame Dienste für Modul-Ansichten — alles, was mehr als ein Modul
/// braucht; Modulspezifisches (eigene Invokes) macht der Deskriptor selbst.
export interface ModuleCtx {
  project: string;
  /// Läuft die Ansicht im eigenen Panel-Fenster (statt angedockt)?
  standalone: boolean;
  toast(msg: string): void;
  /// Dokument in den Entwurfs-Tab laden (Treffer-Klick).
  openDoc(path: string): void;
  /// Wiki-Ziel öffnen (Wikilink, Chip, `tag:`-Namensraum).
  openWiki(name: string): void;
}

export interface PanelTab {
  /// data-mode des Tab-Knopfs und ID-Präfix des Containers (`<mode>-content`,
  /// zugleich CSS-Anker der Ansicht).
  mode: string;
  /// Backend-Modul (enabled_modules) — bestimmt, ob der Tab erscheint.
  module: string;
  /// Tab gehört ins eigene Panel-Fenster: angedockt zieht ein Puffer-Update
  /// den Tab nicht in den Vordergrund (Klick öffnet dort das Fenster).
  popupOnly?: boolean;
  /// Puffer-ID für buffer_read; Updates als `<buffer>-update`.
  buffer: string;
  labelKey: string;
  titleKey: string;
  /// Visueller Trenner hinter dem Tab.
  sepAfter?: boolean;
  /// Baut die Ansicht des Tabs. Ohne init verdrahtet panel-wiring die
  /// Ansicht selbst (Entwurfs-Tab — Kern mit eigenem Markup).
  init?(container: HTMLElement, ctx: ModuleCtx): ModuleView;
  /// Nach Tab-Klick (z. B. Wiki: leere Ansicht lädt die Übersicht).
  onActivate?(view: ModuleView, ctx: ModuleCtx): void;
}

// Der Entwurf ("draft") hat bewusst KEINEN Tab: Die Ansicht erscheint, wenn
// ein Entwurf hereinkommt (panel-update → mode "draft") oder eine Notiz zum
// Bearbeiten geladen wird — als flüchtige Fläche, nicht als Reiter.
export const PANEL_TABS: PanelTab[] = [wikiTab, searchTab, todoTab, commandsTab];
