//! Eigenes StatusNotifierItem (ksni) statt Tauris libappindicator-Tray:
//! libappindicator kann nur ein Menü, wir wollen Linksklick → Popup-Fenster.
//! Die Activate-Koordinaten gehen als Anchor::Click durch; ob sie brauchbar
//! sind, hängt vom SNI-Host ab (Cinnamon ja, KDE 0,0).

use crate::platform::{Anchor, TrayCallbacks};

struct AiControlTray {
  app: tauri::AppHandle,
  icon: Vec<ksni::Icon>,
  cb: TrayCallbacks,
}

impl ksni::Tray for AiControlTray {
  fn id(&self) -> String {
    "ai-control".into()
  }
  fn title(&self) -> String {
    "aICentral".into()
  }
  fn icon_pixmap(&self) -> Vec<ksni::Icon> {
    self.icon.clone()
  }
  fn activate(&mut self, x: i32, y: i32) {
    (self.cb.show)(&self.app, Anchor::Click { x, y });
  }
  fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
    use ksni::menu::StandardItem;
    vec![StandardItem {
      label: "Beenden".into(),
      activate: Box::new(|t: &mut AiControlTray| t.app.exit(0)),
      ..Default::default()
    }
    .into()]
  }
}

/// App-Icon (32×32 PNG) als ARGB32 in Network-Byte-Order für das SNI-IconPixmap.
fn tray_icon_argb() -> Vec<ksni::Icon> {
  let img = tauri::image::Image::from_bytes(include_bytes!("../../../icons/32x32.png"))
    .expect("32x32.png dekodieren");
  let rgba = img.rgba();
  let mut data = Vec::with_capacity(rgba.len());
  for px in rgba.chunks_exact(4) {
    data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
  }
  vec![ksni::Icon {
    width: img.width() as i32,
    height: img.height() as i32,
    data,
  }]
}

pub(super) fn spawn(app: tauri::AppHandle, cb: TrayCallbacks) {
  ksni::TrayService::new(AiControlTray {
    app,
    icon: tray_icon_argb(),
    cb,
  })
  .spawn();
}
