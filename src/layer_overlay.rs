use gtk::prelude::*;
use gtk::{gdk, Align, Box as GtkBox, CssProvider, Label, Orientation, Window, WindowType};

pub struct LayerOverlay {
    window: Window,
    label: Label,
}

pub fn build_layer_overlay() -> Result<LayerOverlay, String> {
    let window = Window::new(WindowType::Toplevel);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_app_paintable(true);
    window.set_accept_focus(false);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);
    window.set_widget_name("focuslock-strip");

    gtk_layer_shell::init_for_window(&window);
    if !gtk_layer_shell::is_layer_window(&window) {
        return Err("gtk-layer-shell failed to initialize".to_string());
    }
    gtk_layer_shell::set_layer(&window, gtk_layer_shell::Layer::Overlay);
    gtk_layer_shell::set_anchor(&window, gtk_layer_shell::Edge::Bottom, true);
    gtk_layer_shell::set_anchor(&window, gtk_layer_shell::Edge::Left, true);
    gtk_layer_shell::set_margin(&window, gtk_layer_shell::Edge::Bottom, 20);
    gtk_layer_shell::set_margin(&window, gtk_layer_shell::Edge::Left, 5);
    gtk_layer_shell::set_exclusive_zone(&window, 0);
    gtk_layer_shell::set_keyboard_mode(&window, gtk_layer_shell::KeyboardMode::None);
    window.set_size_request(-1, 24);

    let label = Label::new(Some("00:00"));
    label.set_widget_name("focuslock-strip-label");
    label.set_xalign(1.0);
    label.set_halign(Align::End);
    label.set_valign(Align::Center);
    label.set_hexpand(true);

    let container = GtkBox::new(Orientation::Horizontal, 0);
    container.set_hexpand(true);
    container.set_vexpand(true);
    container.pack_end(&label, true, true, 0);
    window.add(&container);

    window.connect_realize(|window| {
        if let Some(gdk_window) = window.window() {
            gdk_window.set_pass_through(true);
        }
    });

    let provider = CssProvider::new();
    let css = r#"
#focuslock-strip {
  background: rgba(15, 23, 42, 0.6);
}

#focuslock-strip-label {
  color: #e2e8f0;
  font-family: "IBM Plex Sans", "Noto Sans", sans-serif;
  font-size: 18px;
  font-weight: 600;
  letter-spacing: 0.12em;
  padding: 0 14px;
}
"#;
    provider
        .load_from_data(css.as_bytes())
        .map_err(|err| format!("Failed to load overlay CSS: {err}"))?;
    let screen = gdk::Screen::default().ok_or("Failed to get default screen")?;
    gtk::StyleContext::add_provider_for_screen(
        &screen,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    window.show_all();

    Ok(LayerOverlay { window, label })
}

impl LayerOverlay {
    pub fn set_timer_text(&self, text: &str) {
        self.label.set_text(text);
    }

    pub fn hide(&self) {
        self.window.hide();
    }
}
