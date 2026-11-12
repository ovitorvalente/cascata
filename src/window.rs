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
use crate::preferences;
use crate::settings::Settings;
use crate::tray;

/// Alturas máximas do seletor de qualidade; `None` = melhor disponível.
pub(crate) const QUALITIES: [Option<u32>; 7] = [
    None,
    Some(2160),
    Some(1440),
    Some(1080),
    Some(720),
    Some(480),
    Some(360),
];

pub(crate) fn quality_label(height: Option<u32>) -> String {
    match height {
        None => gettext("Best available"),
        Some(2160) => "2160p (4K)".to_string(),
        Some(height) => format!("{height}p"),
    }
}

/// Posição de uma altura máxima no seletor de qualidade.
pub(crate) fn quality_index(height: Option<u32>) -> u32 {
    QUALITIES
        .iter()
        .position(|other| *other == height)
        .unwrap_or(0) as u32
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

pub(crate) struct Ui {
    pub(crate) window: adw::ApplicationWindow,
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

    pub(crate) settings: RefCell<Settings>,
    tray: RefCell<Option<tray::Tray>>,
    tray_commands: async_channel::Sender<tray::Command>,
    /// Último estado enviado à bandeja, para não repetir atualizações.
    tray_status: RefCell<(bool, String)>,
    /// Se o aviso de "continua em segundo plano" já foi mostrado.
    background_notified: Cell<bool>,
}

pub fn build(app: &adw::Application) {
    // --- Cabeçalho -------------------------------------------------------
    let menu = gio::Menu::new();
    menu.append(
        Some(gettext("Preferences").as_str()),
        Some("win.preferences"),
    );
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

    let settings = Settings::load();
    let (tray_commands, tray_requests) = async_channel::unbounded();

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
        settings: RefCell::new(settings),
        tray: RefCell::new(None),
        tray_commands,
        tray_status: RefCell::new((false, String::new())),
        background_notified: Cell::new(false),
    });
    apply_download_defaults(&ui);

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

    // Ao fechar: com o ícone de bandeja ativo, ou com um download em
    // andamento e a opção de segundo plano ligada, a janela só é escondida.
    ui.window.connect_close_request(clone!(
        #[weak]
        ui,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |window| {
            let busy = ui.job.borrow().is_some();
            let in_tray = ui.tray.borrow().is_some();
            if in_tray || (busy && ui.settings.borrow().background) {
                window.set_visible(false);
                if busy && !in_tray && !ui.background_notified.replace(true) {
                    notify(
                        &ui,
                        NOTIFICATION_BACKGROUND,
                        &gettext("The download continues in the background"),
                        &gettext("Cascata will notify you when it finishes."),
                    );
                }
                return glib::Propagation::Stop;
            }
            // Não deixa um yt-dlp órfão rodando.
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

    let preferences_action = gio::SimpleAction::new("preferences", None);
    preferences_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| preferences::show(&ui)
    ));
    ui.window.add_action(&preferences_action);

    // Fechar pela ação (Ctrl+W) passa pelo mesmo caminho do botão de fechar.
    let close_action = gio::SimpleAction::new("close", None);
    close_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| ui.window.close()
    ));
    ui.window.add_action(&close_action);

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

    // Ações do aplicativo: usadas pelas notificações e pelos atalhos.
    let present_action = gio::SimpleAction::new("present", None);
    present_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| present(&ui)
    ));
    app.add_action(&present_action);

    let quit_action = gio::SimpleAction::new("quit", None);
    quit_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| quit(&ui)
    ));
    app.add_action(&quit_action);

    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.close", &["<Control>w"]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);

    // Pedidos vindos do menu da bandeja. A referência é fraca para que este
    // laço não mantenha a `Ui` viva depois de a janela ser destruída.
    let weak_ui = Rc::downgrade(&ui);
    glib::spawn_future_local(async move {
        while let Ok(command) = tray_requests.recv().await {
            let Some(ui) = weak_ui.upgrade() else {
                break;
            };
            match command {
                tray::Command::Show => present(&ui),
                tray::Command::CancelDownload => {
                    if let Some(job) = ui.job.borrow().as_ref() {
                        job.cancel();
                    }
                }
                tray::Command::Quit => quit(&ui),
            }
        }
    });

    if ui.settings.borrow().tray {
        glib::spawn_future_local(clone!(
            #[weak]
            ui,
            async move {
                // Sem bandeja na sessão (extensão desligada, por exemplo), o
                // app simplesmente funciona sem o ícone.
                if tray::available().await {
                    let _ = start_tray(&ui).await;
                }
            }
        ));
    }

    refresh_deps(&ui);
    ui.window.present();

    // A janela mantém a `Ui` viva; ao ser destruída, a `Ui` é liberada.
    let keep_alive = RefCell::new(Some(ui.clone()));
    ui.window.connect_destroy(move |_| {
        keep_alive.borrow_mut().take();
    });
}

const NOTIFICATION_BACKGROUND: &str = "background";
const NOTIFICATION_DOWNLOAD: &str = "download";

/// Mostra a janela, inclusive quando ela está escondida em segundo plano.
fn present(ui: &Ui) {
    ui.window.set_visible(true);
    ui.window.present();
}

/// Encerra o aplicativo de verdade, mesmo com a bandeja ativa.
fn quit(ui: &Ui) {
    if let Some(job) = ui.job.borrow().as_ref() {
        job.cancel();
    }
    ui.tray.replace(None);
    ui.window.destroy();
}

/// Envia uma notificação do sistema; clicar nela mostra a janela.
fn notify(ui: &Ui, id: &str, title: &str, body: &str) {
    let Some(app) = ui.window.application() else {
        return;
    };
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    notification.set_default_action("app.present");
    app.send_notification(Some(id), &notification);
}

/// Aplica aos seletores da janela os padrões definidos nas preferências.
/// Não mexe em nada durante um download.
pub(crate) fn apply_download_defaults(ui: &Ui) {
    if ui.job.borrow().is_some() {
        return;
    }
    let settings = ui.settings.borrow().clone();
    ui.kind_row.set_selected(match settings.kind {
        Kind::Video => 0,
        Kind::Audio => 1,
    });
    ui.quality_row
        .set_selected(quality_index(settings.max_height));
    ui.quality_row.set_sensitive(settings.kind == Kind::Video);
    set_folder(ui, settings.download_folder());
}

/// Cria o ícone na bandeja, se ainda não existir.
pub(crate) async fn start_tray(ui: &Rc<Ui>) -> Result<(), String> {
    if ui.tray.borrow().is_some() {
        return Ok(());
    }
    let tray = tray::Tray::start(ui.tray_commands.clone()).await?;
    ui.tray.replace(Some(tray));
    // Força o envio do estado atual ao ícone recém-criado.
    let (busy, status) = ui.tray_status.replace((false, String::new()));
    set_tray_status(ui, busy, &status);
    Ok(())
}

/// Remove o ícone da bandeja.
pub(crate) fn stop_tray(ui: &Ui) {
    ui.tray.replace(None);
    // Sem o ícone, uma janela escondida e ociosa ficaria inalcançável.
    if !ui.window.is_visible() && ui.job.borrow().is_none() {
        present(ui);
    }
}

/// Atualiza o estado mostrado no menu da bandeja.
fn set_tray_status(ui: &Ui, busy: bool, status: &str) {
    let state = (busy, status.to_string());
    if *ui.tray_status.borrow() == state {
        return;
    }
    if let Some(tray) = ui.tray.borrow().as_ref() {
        tray.set_status(busy, status);
    }
    ui.tray_status.replace(state);
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
    set_tray_status(ui, true, &gettext("Fetching information…"));

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
            // A bandeja recebe só a porcentagem, que muda poucas vezes.
            set_tray_status(ui, true, &text);
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
            set_tray_status(ui, true, &gettext("Processing with FFmpeg…"));
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
    set_tray_status(ui, false, "");

    // Com a janela escondida ou fora de foco, o resultado vai também para
    // uma notificação do sistema.
    let hidden = !ui.window.is_visible();
    let unattended = hidden || !ui.window.is_active();
    if let Some(app) = ui.window.application() {
        app.withdraw_notification(NOTIFICATION_BACKGROUND);
    }
    ui.background_notified.set(false);

    match outcome {
        Outcome::Done => {
            ui.progress_bar.set_fraction(1.0);
            ui.status_label.set_text(&gettext("Done"));
            ui.url_row.set_text("");
            if unattended {
                notify(
                    ui,
                    NOTIFICATION_DOWNLOAD,
                    &gettext("Download finished"),
                    &ui.title_label.text(),
                );
            }

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
            if unattended {
                notify(
                    ui,
                    NOTIFICATION_DOWNLOAD,
                    &gettext("The download failed"),
                    &message,
                );
            }
            if !hidden {
                show_error(ui, &gettext("The download failed"), &message);
            }
        }
    }

    // Escondido e sem bandeja, não há mais o que fazer em segundo plano. A
    // pequena espera dá tempo de a notificação ser entregue.
    if hidden && ui.tray.borrow().is_none() {
        glib::timeout_add_local_once(
            Duration::from_millis(500),
            clone!(
                #[weak]
                ui,
                move || {
                    if !ui.window.is_visible() && ui.job.borrow().is_none() {
                        ui.window.destroy();
                    }
                }
            ),
        );
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
