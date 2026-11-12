mod about;
mod charts;
mod config;
mod deps;
mod details;
mod downloader;
mod i18n;
mod preferences;
mod settings;
mod stats;
mod tray;
mod window;

use adw::prelude::*;
use gtk::glib;

fn main() -> glib::ExitCode {
    // O idioma escolhido nas preferências vale para o gettext e para o GTK.
    // Precisa ser definido antes de o GTK iniciar e de existir outra thread.
    if let Some(code) = settings::Settings::load().language.code() {
        std::env::set_var("LANGUAGE", code);
    }
    i18n::init();

    let app = adw::Application::builder()
        .application_id(config::APP_ID)
        .build();

    app.connect_activate(|app| {
        // Uma única janela: se já existir, apenas traz para frente.
        // (ela pode estar escondida, com um download em segundo plano).
        if let Some(window) = app.windows().first() {
            window.present();
        } else {
            window::build(app);
        }
    });

    app.run()
}
