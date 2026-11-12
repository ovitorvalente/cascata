mod about;
mod config;
mod deps;
mod downloader;
mod i18n;
mod window;

use adw::prelude::*;
use gtk::glib;

fn main() -> glib::ExitCode {
    i18n::init();

    let app = adw::Application::builder()
        .application_id(config::APP_ID)
        .build();

    app.connect_activate(|app| {
        // Uma única janela: se já existir, apenas traz para frente.
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            window::build(app);
        }
    });

    app.run()
}
