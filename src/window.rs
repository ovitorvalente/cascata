//! Janela principal, montada com os componentes do libadwaita.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib::clone;
use gtk::{gio, glib};

use gettextrs::{gettext, ngettext};

use crate::about;
use crate::config;
use crate::deps;
use crate::downloader::{self, Event, Kind, Outcome, Request};
use crate::i18n::fill;

/// Alturas máximas do seletor de qualidade; `None` = melhor disponível.
const QUALITIES: [Option<u32>; 7] = [
    None,
    Some(2160),
    Some(1440),
    Some(1080),
    Some(720),
    Some(480),
    Some(360),
];

fn quality_label(height: Option<u32>) -> String {
    match height {
        None => gettext("Best available"),
        Some(2160) => "2160p (4K)".to_string(),
        Some(height) => format!("{height}p"),
    }
}

/// Lista legível do que falta, como "yt-dlp and FFmpeg".
fn missing_list(status: deps::Status) -> String {
    match status.missing_names().as_slice() {
        [first, second] => fill(
            &gettext("{first} and {second}"),
            &[("first", first), ("second", second)],
        ),
        names => names.join(", "),
    }
}

struct Ui {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    options: adw::PreferencesGroup,
    url_row: adw::EntryRow,
    kind_row: adw::ComboRow,
    quality_row: adw::ComboRow,
    playlist_row: adw::SwitchRow,
    folder_row: adw::ActionRow,
    download_button: gtk::Button,
    cancel_button: gtk::Button,
    progress_box: gtk::Box,
    progress_bar: gtk::ProgressBar,
    title_label: gtk::Label,
    status_label: gtk::Label,

    folder: RefCell<PathBuf>,
    job: RefCell<Option<downloader::Handle>>,
    last_file: RefCell<Option<String>>,
    pulsing: Cell<bool>,
    installing: Cell<bool>,
}

pub fn build(app: &adw::Application) {
    // --- Cabeçalho -------------------------------------------------------
    let menu = gio::Menu::new();
    menu.append(
        Some(gettext("Check dependencies").as_str()),
        Some("win.check-deps"),
    );
    menu.append(Some(gettext("About Cascata").as_str()), Some("win.about"));

    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(gettext("Main menu"))
        .menu_model(&menu)
        .primary(true)
        .build();

    let header = adw::HeaderBar::new();
    header.pack_end(&menu_button);

    // --- Aviso de dependências ---------------------------------------------
    let banner = adw::Banner::new("");

    // --- Formulário ------------------------------------------------------
    let url_row = adw::EntryRow::builder()
        .title(gettext("Video address"))
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    let paste_button = gtk::Button::builder()
        .icon_name("edit-paste-symbolic")
        .tooltip_text(gettext("Paste"))
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    url_row.add_suffix(&paste_button);

    let source = adw::PreferencesGroup::new();
    source.add(&url_row);

    let kind_row = adw::ComboRow::builder()
        .title(gettext("Format"))
        .model(&gtk::StringList::new(&[
            gettext("Video (MP4)").as_str(),
            gettext("Audio only (MP3)").as_str(),
        ]))
        .build();

    let quality_labels: Vec<String> = QUALITIES.iter().map(|h| quality_label(*h)).collect();
    let quality_labels: Vec<&str> = quality_labels.iter().map(String::as_str).collect();
    let quality_row = adw::ComboRow::builder()
        .title(gettext("Quality"))
        .model(&gtk::StringList::new(&quality_labels))
        .build();

    let playlist_row = adw::SwitchRow::builder()
        .title(gettext("Download the whole playlist"))
        .subtitle(gettext("When the address is part of a playlist"))
        .build();

    let folder_button = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text(gettext("Choose folder"))
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let folder_row = adw::ActionRow::builder()
        .title(gettext("Destination folder"))
        .subtitle_lines(1)
        .activatable_widget(&folder_button)
        .build();
    folder_row.add_suffix(&folder_button);

    let options = adw::PreferencesGroup::builder()
        .title(gettext("Options"))
        .build();
    options.add(&kind_row);
    options.add(&quality_row);
    options.add(&playlist_row);
    options.add(&folder_row);

    // --- Progresso -------------------------------------------------------
    let title_label = gtk::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["heading"])
        .build();
    let progress_bar = gtk::ProgressBar::new();
    let status_label = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label", "caption"])
        .build();

    let progress_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .visible(false)
        .build();
    progress_box.append(&title_label);
    progress_box.append(&progress_bar);
    progress_box.append(&status_label);

    // --- Botões ----------------------------------------------------------
    let download_button = gtk::Button::builder()
        .label(gettext("Download"))
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .build();
    let cancel_button = gtk::Button::builder()
        .label(gettext("Cancel"))
        .halign(gtk::Align::Center)
        .visible(false)
        .css_classes(["pill", "destructive-action"])
        .build();

    // --- Layout ----------------------------------------------------------
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&source);
    content.append(&options);
    content.append(&progress_box);
    content.append(&download_button);
    content.append(&cancel_button);

    let clamp = adw::Clamp::builder()
        .maximum_size(560)
        .child(&content)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&banner);
    body.append(&scrolled);

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&body));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&toasts));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(config::APP_NAME)
        .icon_name(config::APP_ID)
        .default_width(520)
        .default_height(640)
        .content(&toolbar_view)
        .build();

    let default_folder =
        glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(glib::home_dir);

    let ui = Rc::new(Ui {
        window,
        toasts,
        banner,
        options,
        url_row,
        kind_row,
        quality_row,
        playlist_row,
        folder_row,
        download_button,
        cancel_button,
        progress_box,
        progress_bar,
        title_label,
        status_label,
        folder: RefCell::new(PathBuf::new()),
        job: RefCell::new(None),
        last_file: RefCell::new(None),
        pulsing: Cell::new(false),
        installing: Cell::new(false),
    });
    set_folder(&ui, default_folder);

    // --- Sinais ----------------------------------------------------------
    // Os closures guardam referências fracas para não criar ciclos com os
    // widgets que pertencem à própria `Ui`.
    ui.kind_row.connect_selected_notify(clone!(
        #[weak]
        ui,
        move |row| ui.quality_row.set_sensitive(row.selected() == 0)
    ));

    paste_button.connect_clicked(clone!(
        #[weak]
        ui,
        move |_| {
            glib::spawn_future_local(async move {
                if let Ok(Some(text)) = ui.window.clipboard().read_text_future().await {
                    ui.url_row.set_text(text.trim());
                }
            });
        }
    ));

    folder_button.connect_clicked(clone!(
        #[weak]
        ui,
        move |_| choose_folder(&ui)
    ));

    ui.url_row.connect_entry_activated(clone!(
        #[weak]
        ui,
        move |_| {
            if ui.download_button.is_sensitive() && ui.job.borrow().is_none() {
                start_download(&ui);
            }
        }
    ));

    ui.download_button.connect_clicked(clone!(
        #[weak]
        ui,
        move |_| start_download(&ui)
    ));

    ui.cancel_button.connect_clicked(clone!(
        #[weak]
        ui,
        move |button| {
            if let Some(job) = ui.job.borrow().as_ref() {
                button.set_sensitive(false);
                ui.status_label.set_text(&gettext("Cancelling…"));
                job.cancel();
            }
        }
    ));

    ui.banner.connect_button_clicked(clone!(
        #[weak]
        ui,
        move |_| install_deps(&ui)
    ));

    // Não deixa um yt-dlp órfão rodando ao fechar a janela.
    ui.window.connect_close_request(clone!(
        #[weak]
        ui,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_| {
            if let Some(job) = ui.job.borrow().as_ref() {
                job.cancel();
            }
            glib::Propagation::Proceed
        }
    ));

    // --- Ações do menu ---------------------------------------------------
    let about_action = gio::SimpleAction::new("about", None);
    about_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| about::show(&ui.window)
    ));
    ui.window.add_action(&about_action);

    let check_action = gio::SimpleAction::new("check-deps", None);
    check_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| {
            if refresh_deps(&ui).all_present() {
                toast(&ui, &gettext("yt-dlp and FFmpeg are installed"));
            }
        }
    ));
    ui.window.add_action(&check_action);

    refresh_deps(&ui);
    ui.window.present();

    // A janela mantém a `Ui` viva; ao ser destruída, a `Ui` é liberada.
    let keep_alive = RefCell::new(Some(ui.clone()));
    ui.window.connect_destroy(move |_| {
        keep_alive.borrow_mut().take();
    });
}

fn toast(ui: &Ui, message: &str) {
    ui.toasts.add_toast(adw::Toast::new(message));
}

fn set_folder(ui: &Ui, folder: PathBuf) {
    ui.folder_row.set_subtitle(&folder.to_string_lossy());
    ui.folder.replace(folder);
}

/// Atualiza o aviso e os botões conforme o que está instalado.
fn refresh_deps(ui: &Ui) -> deps::Status {
    let status = deps::check();
    let busy = ui.job.borrow().is_some();
    ui.download_button
        .set_sensitive(status.all_present() && !busy);

    if status.all_present() {
        ui.banner.set_revealed(false);
        return status;
    }

    let tools = missing_list(status);
    let count = status.missing_names().len() as u32;

    if deps::install_plan(status, deps::detect_distro()).is_some() {
        let title = ngettext(
            "{tools} is not installed",
            "{tools} are not installed",
            count,
        );
        ui.banner.set_title(&fill(&title, &[("tools", &tools)]));
        ui.banner.set_button_label(Some(&gettext("Install")));
    } else {
        let title = ngettext(
            "{tools} is not installed. Install it with your system's package manager.",
            "{tools} are not installed. Install them with your system's package manager.",
            count,
        );
        ui.banner.set_title(&fill(&title, &[("tools", &tools)]));
        ui.banner.set_button_label(None);
    }
    ui.banner.set_revealed(true);
    status
}

fn install_deps(ui: &Rc<Ui>) {
    if ui.installing.get() {
        return;
    }
    let status = deps::check();
    let Some(plan) = deps::install_plan(status, deps::detect_distro()) else {
        refresh_deps(ui);
        return;
    };

    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let body = gettext(
            "Cascata needs {tools} to work. The command below will run as \
             administrator and you will be asked for your password:\n\n{command}",
        );
        let body = fill(
            &body,
            &[("tools", &missing_list(status)), ("command", &plan.display)],
        );
        let dialog = adw::AlertDialog::new(Some(&gettext("Install dependencies?")), Some(&body));
        let (cancel, install) = (gettext("Cancel"), gettext("Install"));
        dialog.add_responses(&[("cancel", cancel.as_str()), ("install", install.as_str())]);
        dialog.set_response_appearance("install", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("install"));
        dialog.set_close_response("cancel");

        if dialog.choose_future(Some(&ui.window)).await != "install" {
            return;
        }

        ui.installing.set(true);
        ui.banner.set_title(&gettext("Installing dependencies…"));
        ui.banner.set_button_label(None);

        let result = deps::install(plan).await;

        ui.installing.set(false);
        let now = refresh_deps(&ui);

        match result {
            Ok(()) if now.all_present() => toast(&ui, &gettext("Dependencies installed")),
            Ok(()) => show_error(
                &ui,
                &gettext("Installation incomplete"),
                &gettext(
                    "The package manager finished, but the tools still could not \
                     be found in the PATH.",
                ),
            ),
            Err(message) => show_error(&ui, &gettext("Could not install"), &message),
        }
    });
}

fn choose_folder(ui: &Rc<Ui>) {
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Destination folder"))
            .modal(true)
            .initial_folder(&gio::File::for_path(&*ui.folder.borrow()))
            .build();
        // Um erro aqui significa apenas que o usuário fechou o diálogo.
        if let Ok(file) = dialog.select_folder_future(Some(&ui.window)).await {
            if let Some(path) = file.path() {
                set_folder(&ui, path);
            }
        }
    });
}

fn set_busy(ui: &Ui, busy: bool) {
    ui.url_row.set_sensitive(!busy);
    ui.options.set_sensitive(!busy);
    ui.download_button.set_visible(!busy);
    ui.cancel_button.set_visible(busy);
    ui.cancel_button.set_sensitive(true);
}

fn start_download(ui: &Rc<Ui>) {
    if ui.job.borrow().is_some() {
        return;
    }

    let url = ui.url_row.text().trim().to_string();
    if url.is_empty() {
        toast(ui, &gettext("Enter the video address"));
        ui.url_row.grab_focus();
        return;
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        toast(
            ui,
            &gettext("The address must start with http:// or https://"),
        );
        return;
    }
    if !refresh_deps(ui).all_present() {
        return;
    }

    let kind = if ui.kind_row.selected() == 0 {
        Kind::Video
    } else {
        Kind::Audio
    };
    let max_height = QUALITIES
        .get(ui.quality_row.selected() as usize)
        .copied()
        .flatten();

    let request = Request {
        url,
        kind,
        max_height,
        playlist: ui.playlist_row.is_active(),
        folder: ui.folder.borrow().clone(),
    };

    let (handle, events) = match downloader::start(request) {
        Ok(started) => started,
        Err(err) => {
            show_error(ui, &gettext("Could not start yt-dlp"), &err.to_string());
            return;
        }
    };

    ui.job.replace(Some(handle));
    ui.last_file.replace(None);
    ui.pulsing.set(false);
    ui.title_label.set_text(&gettext("Fetching information…"));
    ui.status_label.set_text(&gettext("Connecting…"));
    ui.progress_bar.set_fraction(0.0);
    ui.progress_box.set_visible(true);
    set_busy(ui, true);
    start_pulse(ui);

    let ui = ui.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            handle_event(&ui, event);
        }
    });
}

fn handle_event(ui: &Rc<Ui>, event: Event) {
    match event {
        Event::Title(title) => ui.title_label.set_text(&title),
        Event::Progress {
            fraction,
            speed,
            eta,
        } => {
            ui.pulsing.set(false);
            ui.progress_bar.set_fraction(fraction);
            let percent = format!("{:.0}", fraction * 100.0);
            let mut text = fill(
                &gettext("Downloading — {percent}%"),
                &[("percent", &percent)],
            );
            if !speed.is_empty() {
                text.push_str(&format!(" · {speed}"));
            }
            if !eta.is_empty() {
                text.push_str(" · ");
                text.push_str(&fill(&gettext("{time} left"), &[("time", &eta)]));
            }
            ui.status_label.set_text(&text);
        }
        Event::PostProcessing => {
            ui.status_label
                .set_text(&gettext("Processing with FFmpeg…"));
            start_pulse(ui);
        }
        Event::File(path) => {
            ui.last_file.replace(Some(path));
        }
        Event::Finished(outcome) => finish(ui, outcome),
    }
}

fn finish(ui: &Rc<Ui>, outcome: Outcome) {
    ui.job.replace(None);
    ui.pulsing.set(false);
    set_busy(ui, false);
    refresh_deps(ui);

    match outcome {
        Outcome::Done => {
            ui.progress_bar.set_fraction(1.0);
            ui.status_label.set_text(&gettext("Done"));
            ui.url_row.set_text("");

            let done = adw::Toast::new(&gettext("Download finished"));
            if let Some(path) = ui.last_file.borrow().clone() {
                done.set_button_label(Some(&gettext("Show in folder")));
                done.connect_button_clicked(clone!(
                    #[weak]
                    ui,
                    move |_| {
                        gtk::FileLauncher::new(Some(&gio::File::for_path(&path)))
                            .open_containing_folder(
                                Some(&ui.window),
                                gio::Cancellable::NONE,
                                |_| {},
                            );
                    }
                ));
            }
            ui.toasts.add_toast(done);
        }
        Outcome::Cancelled => {
            ui.progress_bar.set_fraction(0.0);
            ui.status_label.set_text(&gettext("Cancelled"));
        }
        Outcome::Failed(message) => {
            ui.progress_bar.set_fraction(0.0);
            ui.status_label.set_text(&gettext("Failed"));
            show_error(ui, &gettext("The download failed"), &message);
        }
    }
}

/// Anima a barra enquanto não há porcentagem (início e etapa do FFmpeg).
fn start_pulse(ui: &Rc<Ui>) {
    if ui.pulsing.replace(true) {
        return;
    }
    glib::timeout_add_local(
        Duration::from_millis(120),
        clone!(
            #[weak]
            ui,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                if !ui.pulsing.get() {
                    return glib::ControlFlow::Break;
                }
                ui.progress_bar.pulse();
                glib::ControlFlow::Continue
            }
        ),
    );
}

fn show_error(ui: &Ui, heading: &str, message: &str) {
    let dialog = adw::AlertDialog::new(Some(heading), Some(message));
    dialog.add_response("close", &gettext("Close"));
    dialog.present(Some(&ui.window));
}
