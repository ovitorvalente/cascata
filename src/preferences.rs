//! Diálogo de preferências.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib::clone;
use gtk::{gio, glib};

use crate::downloader::Kind;
use crate::settings::Language;
use crate::tray::{self, ExtensionOutcome};
use crate::window::{self, Ui, QUALITIES};

pub fn show(ui: &Rc<Ui>) {
    let settings = ui.settings.borrow().clone();

    // --- Geral -----------------------------------------------------------
    let language_row = adw::ComboRow::builder()
        .title(gettext("Language"))
        .subtitle(gettext("Takes effect the next time Cascata starts"))
        .model(&gtk::StringList::new(&[
            gettext("System default").as_str(),
            // Nomes de idiomas ficam sempre no próprio idioma.
            "English",
            "Português (Brasil)",
        ]))
        .selected(position(&Language::ALL, settings.language))
        .build();

    let general = adw::PreferencesGroup::builder()
        .title(gettext("General"))
        .build();
    general.add(&language_row);

    // --- Downloads -------------------------------------------------------
    let kind_row = adw::ComboRow::builder()
        .title(gettext("Format"))
        .model(&gtk::StringList::new(&[
            gettext("Video (MP4)").as_str(),
            gettext("Audio only (MP3)").as_str(),
        ]))
        .selected(match settings.kind {
            Kind::Video => 0,
            Kind::Audio => 1,
        })
        .build();

    let quality_labels: Vec<String> = QUALITIES
        .iter()
        .map(|height| window::quality_label(*height))
        .collect();
    let quality_labels: Vec<&str> = quality_labels.iter().map(String::as_str).collect();
    let quality_row = adw::ComboRow::builder()
        .title(gettext("Quality"))
        .model(&gtk::StringList::new(&quality_labels))
        .selected(window::quality_index(settings.max_height))
        .sensitive(settings.kind == Kind::Video)
        .build();

    let folder_button = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text(gettext("Choose folder"))
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let folder_row = adw::ActionRow::builder()
        .title(gettext("Destination folder"))
        .subtitle(settings.download_folder().to_string_lossy())
        .subtitle_lines(1)
        .activatable_widget(&folder_button)
        .build();
    folder_row.add_suffix(&folder_button);

    let downloads = adw::PreferencesGroup::builder()
        .title(gettext("Downloads"))
        .description(gettext("Initial choices for new downloads"))
        .build();
    downloads.add(&kind_row);
    downloads.add(&quality_row);
    downloads.add(&folder_row);

    // --- Segundo plano ---------------------------------------------------
    let background_row = adw::SwitchRow::builder()
        .title(gettext("Keep downloading in the background"))
        .subtitle(gettext(
            "Closing the window does not stop a download; a notification tells you when it finishes",
        ))
        .active(settings.background)
        .build();
    let tray_row = adw::SwitchRow::builder()
        .title(gettext("Tray icon"))
        .subtitle(gettext(
            "Keep Cascata in the system tray after the window is closed",
        ))
        .active(settings.tray)
        .build();

    let background = adw::PreferencesGroup::builder()
        .title(gettext("Background"))
        .build();
    background.add(&background_row);
    background.add(&tray_row);

    let page = adw::PreferencesPage::new();
    page.add(&general);
    page.add(&downloads);
    page.add(&background);

    let dialog = adw::PreferencesDialog::builder()
        .title(gettext("Preferences"))
        .build();
    dialog.add(&page);

    // --- Sinais ----------------------------------------------------------
    language_row.connect_selected_notify(clone!(
        #[weak]
        ui,
        #[weak]
        dialog,
        move |row| {
            let language = Language::ALL
                .get(row.selected() as usize)
                .copied()
                .unwrap_or_default();
            ui.settings.borrow_mut().language = language;
            save(&ui, &dialog);
            dialog.add_toast(adw::Toast::new(&gettext(
                "Restart Cascata to apply the language",
            )));
        }
    ));

    kind_row.connect_selected_notify(clone!(
        #[weak]
        ui,
        #[weak]
        dialog,
        #[weak]
        quality_row,
        move |row| {
            let kind = if row.selected() == 0 {
                Kind::Video
            } else {
                Kind::Audio
            };
            quality_row.set_sensitive(kind == Kind::Video);
            ui.settings.borrow_mut().kind = kind;
            save(&ui, &dialog);
            window::apply_download_defaults(&ui);
        }
    ));

    quality_row.connect_selected_notify(clone!(
        #[weak]
        ui,
        #[weak]
        dialog,
        move |row| {
            let height = QUALITIES.get(row.selected() as usize).copied().flatten();
            ui.settings.borrow_mut().max_height = height;
            save(&ui, &dialog);
            window::apply_download_defaults(&ui);
        }
    ));

    folder_button.connect_clicked(clone!(
        #[weak]
        ui,
        #[weak]
        dialog,
        #[weak]
        folder_row,
        move |_| {
            glib::spawn_future_local(async move {
                let current = ui.settings.borrow().download_folder();
                let chooser = gtk::FileDialog::builder()
                    .title(gettext("Destination folder"))
                    .modal(true)
                    .initial_folder(&gio::File::for_path(current))
                    .build();
                // Um erro aqui significa apenas que o usuário fechou o diálogo.
                let Ok(file) = chooser.select_folder_future(Some(&ui.window)).await else {
                    return;
                };
                let Some(path) = file.path() else {
                    return;
                };
                folder_row.set_subtitle(&path.to_string_lossy());
                ui.settings.borrow_mut().folder = Some(path);
                save(&ui, &dialog);
                window::apply_download_defaults(&ui);
            });
        }
    ));

    background_row.connect_active_notify(clone!(
        #[weak]
        ui,
        #[weak]
        dialog,
        move |row| {
            ui.settings.borrow_mut().background = row.is_active();
            save(&ui, &dialog);
        }
    ));

    // Evita reagir quando o próprio código devolve a chave à posição anterior.
    let reverting = Rc::new(Cell::new(false));
    tray_row.connect_active_notify(clone!(
        #[weak]
        ui,
        #[weak]
        dialog,
        move |row| {
            if reverting.get() {
                return;
            }
            let wanted = row.is_active();
            let row = row.clone();
            let reverting = reverting.clone();
            glib::spawn_future_local(async move {
                row.set_sensitive(false);
                let enabled = if wanted {
                    enable_tray(&ui, &dialog).await
                } else {
                    window::stop_tray(&ui);
                    false
                };
                row.set_sensitive(true);

                if row.is_active() != enabled {
                    reverting.set(true);
                    row.set_active(enabled);
                    reverting.set(false);
                }
                ui.settings.borrow_mut().tray = enabled;
                save(&ui, &dialog);
            });
        }
    ));

    dialog.present(Some(&ui.window));
}

fn position<T: PartialEq>(items: &[T], item: T) -> u32 {
    items.iter().position(|other| *other == item).unwrap_or(0) as u32
}

fn save(ui: &Ui, dialog: &adw::PreferencesDialog) {
    let result = ui.settings.borrow().save();
    if let Err(message) = result {
        let text = format!("{} {message}", gettext("Could not save the preferences:"));
        dialog.add_toast(adw::Toast::new(&text));
    }
}

fn show_error(dialog: &adw::PreferencesDialog, heading: &str, message: &str) {
    let alert = adw::AlertDialog::new(Some(heading), Some(message));
    alert.add_response("close", &gettext("Close"));
    alert.present(Some(dialog));
}

/// Liga o ícone de bandeja, oferecendo instalar a extensão do GNOME Shell
/// quando não há bandeja na sessão. Retorna se o ícone ficou ativo.
async fn enable_tray(ui: &Rc<Ui>, dialog: &adw::PreferencesDialog) -> bool {
    if !tray::available().await {
        let alert = adw::AlertDialog::new(
            Some(&gettext("Install the tray extension?")),
            Some(&gettext(
                "GNOME does not show tray icons on its own. Cascata can install and \
                 enable the “AppIndicator and KStatusNotifierItem Support” extension \
                 from extensions.gnome.org. GNOME will ask you to confirm the \
                 installation.",
            )),
        );
        let (cancel, install) = (gettext("Cancel"), gettext("Install and enable"));
        alert.add_responses(&[("cancel", cancel.as_str()), ("install", install.as_str())]);
        alert.set_response_appearance("install", adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some("install"));
        alert.set_close_response("cancel");

        if alert.choose_future(Some(dialog)).await != "install" {
            return false;
        }

        dialog.add_toast(adw::Toast::new(&gettext(
            "Waiting for GNOME to install the extension…",
        )));

        match tray::enable_extension().await {
            ExtensionOutcome::Enabled => {
                if !tray::wait_until_available(15).await {
                    show_error(
                        dialog,
                        &gettext("The tray is not available yet"),
                        &gettext(
                            "The extension is installed, but GNOME has not started it. \
                             Log out and log in again, then turn this option on.",
                        ),
                    );
                    return false;
                }
            }
            ExtensionOutcome::Cancelled => return false,
            ExtensionOutcome::Failed(message) => {
                let hint = gettext(
                    "You can install “AppIndicator and KStatusNotifierItem Support” \
                     yourself from extensions.gnome.org.",
                );
                show_error(
                    dialog,
                    &gettext("Could not install the extension"),
                    &format!("{message}\n\n{hint}"),
                );
                return false;
            }
        }
    }

    match window::start_tray(ui).await {
        Ok(()) => true,
        Err(message) => {
            show_error(dialog, &gettext("Could not create the tray icon"), &message);
            false
        }
    }
}
