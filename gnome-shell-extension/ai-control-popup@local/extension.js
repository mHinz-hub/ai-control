/* GNOME Shell Extension (ESM, GNOME 45+)
 * ai-control-popup@local
 * Dünner Relay: Panel-Button -> D-Bus Show() an die ai-control-App, dann das
 * rahmenlose Popup-Fenster der App unter den Button schieben (Compositor darf das).
 */

import St from 'gi://St';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

const DBUS_NAME = 'com.aicontrol.Popup';
const DBUS_PATH = '/com/aicontrol/Popup';
const DBUS_IFACE = 'com.aicontrol.Popup1';
const WIN_TITLE = 'ai-control-popup';

export default class AiControlPopupExtension extends Extension {
  enable() {
    this._proxy = Gio.DBusProxy.new_for_bus_sync(
      Gio.BusType.SESSION,
      Gio.DBusProxyFlags.DO_NOT_AUTO_START,
      null,
      DBUS_NAME,
      DBUS_PATH,
      DBUS_IFACE,
      null,
    );

    // Beim Mappen unsichtbar schalten und unmittelbar vor dem nächsten Redraw
    // positionieren. Der BEFORE_REDRAW-Later läuft garantiert, bevor der erste
    // Frame gemalt wird — der erste sichtbare Frame sitzt damit schon am Zielort,
    // kein zentrierter Zwischenframe.
    this._mapId = global.window_manager.connect('map', (_wm, actor) => {
      const w = actor.meta_window;
      if (!w || w.get_title() !== WIN_TITLE) return;
      actor.remove_all_transitions();
      actor.opacity = 0;
      this._placeBeforeRedraw(actor, w);
    });

    // Panel-Button nur zeigen, solange die App den D-Bus-Namen hält.
    this._watchId = Gio.bus_watch_name(
      Gio.BusType.SESSION,
      DBUS_NAME,
      Gio.BusNameWatcherFlags.NONE,
      () => this._addButton(),
      () => this._removeButton(),
    );
  }

  _addButton() {
    if (this._button) return;
    // dontCreateMenu=true: wir wollen kein Shell-Menü, nur den Klick relayen.
    this._button = new PanelMenu.Button(0.0, 'ai-control', true);
    const icon = new St.Icon({
      // Icon liegt neben der Extension (vom Paket mitgeliefert).
      gicon: Gio.icon_new_for_string(`${this.path}/trayLinux.png`),
      icon_size: 36,
    });
    this._button.add_child(icon);
    this._button.connect('button-press-event', () => {
      this._toggle();
      return Clutter.EVENT_STOP;
    });
    Main.panel.addToStatusArea('ai-control-popup', this._button, 0, 'right');
  }

  _removeButton() {
    this._button?.destroy();
    this._button = null;
  }

  _toggle() {
    const win = this._findWindow();
    if (win && win.showing_on_its_workspace()) {
      this._call('Hide');
      this._disconnectOutsideClick();
      return;
    }
    // Fenster unsichtbar halten, bis es am Platz sitzt (kein Sprung Mitte->Tray).
    const actor = win ? win.get_compositor_private() : null;
    if (actor) actor.opacity = 0;
    this._call('Show');
    this._positionWhenReady();
    this._connectOutsideClick();
  }

  // Klick auf leere Desktop-Fläche oder Shell-Chrome wechselt den Fensterfokus
  // nicht — der Blur-Pfad der App greift dort nicht. Solche Klicks landen im
  // Shell-Stage: solange das Popup offen ist, hier abgreifen und Hide relayen.
  _connectOutsideClick() {
    if (this._clickId) return;
    this._clickId = global.stage.connect('captured-event', (_stage, event) => {
      if (event.type() !== Clutter.EventType.BUTTON_PRESS) return Clutter.EVENT_PROPAGATE;
      const win = this._findWindow();
      if (!win || !win.showing_on_its_workspace()) {
        this._disconnectOutsideClick();
        return Clutter.EVENT_PROPAGATE;
      }
      const [x, y] = event.get_coords();
      // Panel-Button toggelt selbst; Klicks ins Popup erreichen den Stage nicht.
      const [bx, by] = this._button.get_transformed_position();
      if (x >= bx && x < bx + this._button.width && y >= by && y < by + this._button.height) {
        return Clutter.EVENT_PROPAGATE;
      }
      this._call('Hide');
      this._disconnectOutsideClick();
      return Clutter.EVENT_PROPAGATE;
    });
  }

  _disconnectOutsideClick() {
    if (!this._clickId) return;
    global.stage.disconnect(this._clickId);
    this._clickId = 0;
  }

  _call(method) {
    try {
      this._proxy.call_sync(method, null, Gio.DBusCallFlags.NONE, -1, null);
    } catch (e) {
      logError(e, 'ai-control-popup: D-Bus call failed (App läuft?)');
    }
  }

  _position(win) {
    const [bx, by] = this._button.get_transformed_position();
    const bw = this._button.width;
    const bh = this._button.height;
    const rect = win.get_frame_rect();
    const mon = Main.layoutManager.primaryMonitor;
    let x = Math.round(bx + bw - rect.width);
    if (x < mon.x) x = mon.x;
    // Anker ist die Button-Unterkante, nicht die Work-Area: eine auto-versteckte
    // Leiste (hidetopbar) reserviert keine Struts, deren Work-Area-y wäre immer 0.
    const y = Math.round(by + bh);
    // Höhe an der Monitor-Unterkante klemmen, sonst schiebt der Compositor ein zu
    // hohes Fenster nach oben aus dem Sichtbereich (oberer Teil verschwindet).
    const h = Math.min(rect.height, mon.y + mon.height - y);
    win.move_resize_frame(true, x, y, rect.width, h);
  }

  _placeBeforeRedraw(actor, w) {
    const laters = global.compositor.get_laters();
    laters.add(Meta.LaterType.BEFORE_REDRAW, () => {
      const rect = w.get_frame_rect();
      // Noch keine echte Größe -> nächster Redraw, bis positionierbar.
      if (!rect || rect.width <= 1) return GLib.SOURCE_CONTINUE;
      this._position(w);
      actor.opacity = 255;
      return GLib.SOURCE_REMOVE;
    });
  }

  _positionWhenReady() {
    if (this._moveId) GLib.source_remove(this._moveId);
    let waited = 0;
    let held = 0;
    // Erst positionieren, wenn das Fenster gemappt ist UND eine echte Größe hat
    // (frisch gezeigt liefert get_frame_rect kurz 0 -> off-screen beim 1. Klick).
    // Danach ~400 ms weiter nachziehen, falls der Compositor nachplatziert.
    let placed = false;
    this._moveId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 25, () => {
      const win = this._findWindow();
      const actor = win ? win.get_compositor_private() : null;
      const rect = win ? win.get_frame_rect() : null;
      const ready = win && win.showing_on_its_workspace() && rect && rect.width > 1;
      if (ready) {
        this._position(win);
        // Erst nach dem Positionieren im selben Frame sichtbar schalten.
        if (!placed) {
          placed = true;
          if (actor) actor.opacity = 255;
        }
        held += 25;
        if (held >= 400) {
          this._moveId = 0;
          return GLib.SOURCE_REMOVE;
        }
        return GLib.SOURCE_CONTINUE;
      }
      // Noch nicht platziert -> unsichtbar halten.
      if (actor) actor.opacity = 0;
      waited += 25;
      if (waited > 1500) {
        // Nicht platzierbar: trotzdem sichtbar schalten, nie unsichtbar hängen lassen.
        if (actor) actor.opacity = 255;
        this._moveId = 0;
        return GLib.SOURCE_REMOVE;
      }
      return GLib.SOURCE_CONTINUE;
    });
  }

  _findWindow() {
    for (const actor of global.get_window_actors()) {
      const w = actor.meta_window;
      if (w && w.get_title() === WIN_TITLE) return w;
    }
    return null;
  }

  disable() {
    if (this._watchId) {
      Gio.bus_unwatch_name(this._watchId);
      this._watchId = 0;
    }
    if (this._moveId) {
      GLib.source_remove(this._moveId);
      this._moveId = 0;
    }
    if (this._mapId) {
      global.window_manager.disconnect(this._mapId);
      this._mapId = 0;
    }
    this._disconnectOutsideClick();
    this._removeButton();
    this._proxy = null;
  }
}
