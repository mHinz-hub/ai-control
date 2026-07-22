/* Cinnamon-Extension (CJS, Muffin/Meta-0)
 * ai-control-popup@local
 *
 * Das Tray-Icon kommt unter Cinnamon über SNI (xapp), die App zeigt das
 * Popup selbst. Die Extension ergänzt, was der Client nicht kann:
 * - Wayland: set_position aus dem Client ist wirkungslos — die Extension
 *   schiebt das frisch gemappte Popup an den Mauszeiger (der beim Klick auf
 *   dem Icon steht), mit derselben Klemm-Logik wie position_popup in app.rs.
 * - X11: Muffins Focus-Stealing-Prevention verwirft das present() der App —
 *   ohne initialen Fokus schließt das Blur-Verstecken nie. Die Extension
 *   fokussiert das Popup beim Map aus dem Compositor heraus (platziert wird
 *   client-seitig über die SNI-Activate-Koordinaten).
 */

const Meta = imports.gi.Meta;

const WIN_TITLE = 'ai-control-popup';

let mapId = 0;

function _place(win) {
  const [px, py] = global.get_pointer();
  const rect = win.get_frame_rect();
  // Work-Area statt Monitor: die Panel-Struts bleiben frei, das Popup sitzt
  // über der Leiste statt darauf.
  const wa = win.get_work_area_current_monitor();
  const maxX = wa.x + wa.width - rect.width;
  const maxY = wa.y + wa.height - rect.height;
  const x = Math.min(Math.max(px - rect.width, wa.x), Math.max(maxX, wa.x));
  const y = Math.min(Math.max(py, wa.y), Math.max(maxY, wa.y));
  win.move_frame(true, x, y);
}

// Beim Mappen unsichtbar schalten und vor dem nächsten Redraw positionieren —
// der erste sichtbare Frame sitzt schon am Zielort (Muster wie in der
// GNOME-Extension, dort über global.compositor.get_laters()).
function _placeBeforeRedraw(actor, win) {
  actor.remove_all_transitions();
  actor.opacity = 0;
  Meta.later_add(Meta.LaterType.BEFORE_REDRAW, () => {
    const rect = win.get_frame_rect();
    // Noch keine echte Größe -> nächster Redraw, bis positionierbar.
    if (!rect || rect.width <= 1) return true;
    _place(win);
    actor.opacity = 255;
    return false;
  });
}

function init(_metadata) {}

function enable() {
  mapId = global.window_manager.connect('map', (_wm, actor) => {
    const win = actor.meta_window;
    if (!win || win.get_title() !== WIN_TITLE) return;
    if (Meta.is_wayland_compositor()) {
      _placeBeforeRedraw(actor, win);
    } else {
      win.activate(global.get_current_time());
    }
  });
}

function disable() {
  if (mapId) {
    global.window_manager.disconnect(mapId);
    mapId = 0;
  }
}
