//! Janela principal, montada com os componentes do libadwaita.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib::clone;
use gtk::{gio, glib};

use crate::about;
use crate::config;
use crate::deps;
use crate::downloader::{self, Event, Kind, Outcome, Request};

/// Opções do seletor de qualidade: (rótulo, altura máxima).
const QUALITIES: [(&str, Option<u32>); 7] = [
    ("Melhor disponível", None),
    ("2160p (4K)", Some(2160)),
    ("1440p", Some(1440)),
    ("1080p", Some(1080)),
    ("720p", Some(720)),
    ("480p", Some(480)),
    ("360p", Some(360)),
];

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
    menu.append(Some("Verificar dependências"), Some("win.check-deps"));
    menu.append(Some("Sobre o Cascata"), Some("win.about"));

    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Menu principal")
        .menu_model(&menu)
        .primary(true)
        .build();

    let header = adw::HeaderBar::new();
    header.pack_end(&menu_button);

    // --- Aviso de dependências ---------------------------------------------
    let banner = adw::Banner::new("");

    // --- Formulário ------------------------------------------------------
    let url_row = adw::EntryRow::builder()
        .title("Endereço do vídeo")
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    let paste_button = gtk::Button::builder()
        .icon_name("edit-paste-symbolic")
        .tooltip_text("Colar")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    url_row.add_suffix(&paste_button);

    let source = adw::PreferencesGroup::new();
    source.add(&url_row);

    let kind_row = adw::ComboRow::builder()
        .title("Formato")
        .model(&gtk::StringList::new(&[
            "Vídeo (MP4)",
            "Somente áudio (MP3)",
        ]))
        .build();

    let quality_labels: Vec<&str> = QUALITIES.iter().map(|(label, _)| *label).collect();
    let quality_row = adw::ComboRow::builder()
        .title("Qualidade")
        .model(&gtk::StringList::new(&quality_labels))
        .build();

    let playlist_row = adw::SwitchRow::builder()
        .title("Baixar a playlist inteira")
        .subtitle("Quando o endereço fizer parte de uma playlist")
        .build();

    let folder_button = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text("Escolher pasta")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let folder_row = adw::ActionRow::builder()
        .title("Pasta de destino")
        .subtitle_lines(1)
        .activatable_widget(&folder_button)
        .build();
    folder_row.add_suffix(&folder_button);

    let options = adw::PreferencesGroup::builder().title("Opções").build();
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
        .label("Baixar")
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .build();
    let cancel_button = gtk::Button::builder()
        .label("Cancelar")
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
                ui.status_label.set_text("Cancelando…");
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
                toast(&ui, "yt-dlp e FFmpeg estão instalados");
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

    let missing = status.missing_names().join(" e ");
    let verb = if status.missing_names().len() > 1 {
        "não estão instalados"
    } else {
        "não está instalado"
    };

    if deps::install_plan(status, deps::detect_distro()).is_some() {
        ui.banner.set_title(&format!("{missing} {verb}"));
        ui.banner.set_button_label(Some("Instalar"));
    } else {
        ui.banner.set_title(&format!(
            "{missing} {verb}. Instale pelo gerenciador de pacotes do seu sistema."
        ));
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
        let dialog = adw::AlertDialog::new(
            Some("Instalar dependências?"),
            Some(&format!(
                "O Cascata precisa de {} para funcionar. O comando abaixo será \
                 executado como administrador e a sua senha será solicitada:\n\n{}",
                status.missing_names().join(" e "),
                plan.display
            )),
        );
        dialog.add_responses(&[("cancel", "Cancelar"), ("install", "Instalar")]);
        dialog.set_response_appearance("install", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("install"));
        dialog.set_close_response("cancel");

        if dialog.choose_future(Some(&ui.window)).await != "install" {
            return;
        }

        ui.installing.set(true);
        ui.banner.set_title("Instalando dependências…");
        ui.banner.set_button_label(None);

        let result = deps::install(plan).await;

        ui.installing.set(false);
        let now = refresh_deps(&ui);

        match result {
            Ok(()) if now.all_present() => toast(&ui, "Dependências instaladas"),
            Ok(()) => show_error(
                &ui,
                "Instalação incompleta",
                "O gerenciador de pacotes terminou, mas as ferramentas ainda não \
                 foram encontradas no PATH.",
            ),
            Err(message) => show_error(&ui, "Não foi possível instalar", &message),
        }
    });
}

fn choose_folder(ui: &Rc<Ui>) {
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let dialog = gtk::FileDialog::builder()
            .title("Pasta de destino")
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
        toast(ui, "Informe o endereço do vídeo");
        ui.url_row.grab_focus();
        return;
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        toast(ui, "O endereço precisa começar com http:// ou https://");
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
        .and_then(|(_, height)| *height);

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
            show_error(ui, "Não foi possível iniciar o yt-dlp", &err.to_string());
            return;
        }
    };

    ui.job.replace(Some(handle));
    ui.last_file.replace(None);
    ui.pulsing.set(false);
    ui.title_label.set_text("Obtendo informações…");
    ui.status_label.set_text("Conectando…");
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
            let mut text = format!("Baixando — {:.0}%", fraction * 100.0);
            if !speed.is_empty() {
                text.push_str(&format!(" · {speed}"));
            }
            if !eta.is_empty() {
                text.push_str(&format!(" · restam {eta}"));
            }
            ui.status_label.set_text(&text);
        }
        Event::PostProcessing => {
            ui.status_label.set_text("Processando com o FFmpeg…");
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
            ui.status_label.set_text("Concluído");
            ui.url_row.set_text("");

            let done = adw::Toast::new("Download concluído");
            if let Some(path) = ui.last_file.borrow().clone() {
                done.set_button_label(Some("Mostrar na pasta"));
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
            ui.status_label.set_text("Cancelado");
        }
        Outcome::Failed(message) => {
            ui.progress_bar.set_fraction(0.0);
            ui.status_label.set_text("Falhou");
            show_error(ui, "O download falhou", &message);
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
    dialog.add_response("close", "Fechar");
    dialog.present(Some(&ui.window));
}
