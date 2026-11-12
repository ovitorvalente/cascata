//! Janela principal, montada com os componentes do libadwaita.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib::clone;
use gtk::{gdk, gio, glib};

use gettextrs::{gettext, ngettext};

use crate::about;
use crate::charts;
use crate::config;
use crate::deps;
use crate::details;
use crate::downloader::{self, Event, Kind, Outcome, Request};
use crate::i18n::fill;
use crate::preferences;
use crate::settings::Settings;
use crate::stats::{self, Phase, Session};
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
    chart: gtk::DrawingArea,

    folder: RefCell<PathBuf>,
    job: RefCell<Option<downloader::Handle>>,
    /// Estado do download atual, compartilhado com os gráficos.
    pub(crate) session: Rc<RefCell<Session>>,
    pub(crate) details: RefCell<Option<details::Details>>,
    pub(crate) thumbnail: RefCell<Option<gdk::Texture>>,
    pulsing: Cell<bool>,
    installing: Cell<bool>,

    pub(crate) settings: RefCell<Settings>,
    tray: RefCell<Option<tray::Tray>>,
    tray_commands: async_channel::Sender<tray::Command>,
    /// Fase e porcentagem da última atualização enviada à bandeja.
    tray_key: Cell<Option<(Phase, Option<u8>, bool)>>,
    tray_pushed: Cell<Option<Instant>>,
    /// Mostra por alguns segundos o ícone de concluído ou de falha.
    tray_flash: Cell<bool>,
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
    menu.append(
        Some(gettext("Download details").as_str()),
        Some("win.details"),
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
    let session = Rc::new(RefCell::new(Session::default()));
    let chart = charts::speed_chart(&session, 110);
    chart.set_margin_top(4);

    progress_box.append(&title_label);
    progress_box.append(&progress_bar);
    progress_box.append(&status_label);
    progress_box.append(&chart);

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
        chart,
        folder: RefCell::new(PathBuf::new()),
        job: RefCell::new(None),
        session,
        details: RefCell::new(None),
        thumbnail: RefCell::new(None),
        pulsing: Cell::new(false),
        installing: Cell::new(false),
        settings: RefCell::new(settings),
        tray: RefCell::new(None),
        tray_commands,
        tray_key: Cell::new(None),
        tray_pushed: Cell::new(None),
        tray_flash: Cell::new(false),
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
        move |_| cancel_download(&ui)
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
            release_focus(&ui);
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
            // A janela principal vai ser destruída; o painel vai junto.
            destroy_details(&ui);
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

    let details_action = gio::SimpleAction::new("details", None);
    details_action.connect_activate(clone!(
        #[weak]
        ui,
        move |_, _| details::show(&ui)
    ));
    ui.window.add_action(&details_action);

    // As cores do gráfico mudam com o tema.
    adw::StyleManager::default().connect_dark_notify(clone!(
        #[weak]
        ui,
        move |_| ui.chart.queue_draw()
    ));

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
                tray::Command::ShowDetails => details::show(&ui),
                tray::Command::ShowLastFile => show_last_file(&ui),
                tray::Command::CancelDownload => cancel_download(&ui),
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

    // O aplicativo mantém a `Ui` viva até encerrar. (O sinal "destroy" da
    // janela não serve para isso: no GTK 4 ele só dispara depois que a
    // última referência à janela é liberada.)
    let keep_alive = RefCell::new(Some(ui.clone()));
    app.connect_shutdown(move |_| {
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
    destroy_windows(ui);
}

/// Fecha o painel de detalhes de vez. Ele é uma janela do aplicativo: se
/// continuasse existindo, o aplicativo não encerraria.
fn destroy_details(ui: &Ui) {
    if let Some(details) = ui.details.take() {
        details.window.destroy();
    }
}

/// Destrói todas as janelas, o que encerra o aplicativo.
fn destroy_windows(ui: &Ui) {
    destroy_details(ui);
    release_focus(ui);
    ui.window.destroy();
}

/// Tira o foco do campo de endereço. Sem isso, o GTK reclama quando o
/// campo é desativado, ou a janela destruída, com ele ainda em foco.
fn release_focus(ui: &Ui) {
    gtk::prelude::GtkWindowExt::set_focus(&ui.window, gtk::Widget::NONE);
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
    refresh_tray(ui, true);
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

/// Nome curto de uma pasta para exibição: "~/Vídeos" em vez do caminho
/// completo dentro da pasta pessoal.
fn display_folder(folder: &std::path::Path) -> String {
    match folder.strip_prefix(glib::home_dir()) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => folder.to_string_lossy().into_owned(),
    }
}

/// Encurta um texto para caber em uma linha de menu.
fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let head: String = text.chars().take(limit.saturating_sub(1)).collect();
        format!("{}…", head.trim_end())
    }
}

/// Monta o que a bandeja deve mostrar para o estado atual.
fn tray_view(ui: &Ui) -> tray::View {
    let session = ui.session.borrow();
    let percent = session.percent();

    let icon = match session.phase {
        Phase::Starting => Some(charts::TrayIcon::Busy),
        Phase::Downloading => {
            Some(percent.map_or(charts::TrayIcon::Busy, charts::TrayIcon::Progress))
        }
        Phase::Processing => Some(charts::TrayIcon::Progress(100)),
        Phase::Done if ui.tray_flash.get() => Some(charts::TrayIcon::Done),
        Phase::Failed if ui.tray_flash.get() => Some(charts::TrayIcon::Failed),
        _ => None,
    };

    let mut lines = Vec::new();
    let mut summary = String::new();
    if session.phase.is_active() {
        if !session.info.title.is_empty() {
            lines.push(shorten(&session.info.title, 48));
        }
        match (session.phase, session.progress.as_ref()) {
            (Phase::Downloading, Some(progress)) => {
                let percent = percent.unwrap_or(0);
                lines.push(format!(
                    "{} {percent}%",
                    stats::text_bar(progress.fraction, 12)
                ));
                let mut numbers = vec![stats::size_text(progress)];
                let rate = stats::rate_text(progress);
                if !rate.is_empty() {
                    numbers.push(rate.clone());
                }
                lines.push(numbers.join(" · "));
                summary = format!("{percent}%");
                if !rate.is_empty() {
                    summary = format!("{summary} · {rate}");
                }
            }
            _ => summary = session.activity(),
        }
        let mut activity = session.activity();
        if let Some(playlist) = session.playlist_text() {
            activity = format!("{activity} · {playlist}");
        }
        lines.push(activity);
        lines.push(format!("{} → {}", session.format, session.folder));
    }

    tray::View {
        icon: icon.map(charts::tray_icon).unwrap_or_default(),
        summary,
        sparkline: if session.phase == Phase::Downloading {
            charts::sparkline_png(&session.samples)
        } else {
            Vec::new()
        },
        lines,
        can_cancel: session.phase.is_active(),
        last_file: session.last_file.as_ref().map(|path| {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            shorten(&name, 40)
        }),
    }
}

/// Atualiza o ícone e o menu da bandeja. Fora de mudanças de fase ou de
/// porcentagem, envia no máximo uma atualização por segundo.
fn refresh_tray(ui: &Ui, force: bool) {
    if ui.tray.borrow().is_none() {
        return;
    }
    let key = {
        let session = ui.session.borrow();
        (session.phase, session.percent(), ui.tray_flash.get())
    };
    let recent = ui
        .tray_pushed
        .get()
        .is_some_and(|pushed| pushed.elapsed() < Duration::from_secs(1));
    if !force && ui.tray_key.get() == Some(key) && recent {
        return;
    }
    let view = tray_view(ui);
    if let Some(tray) = ui.tray.borrow().as_ref() {
        tray.set_view(view);
    }
    ui.tray_key.set(Some(key));
    ui.tray_pushed.set(Some(Instant::now()));
}

/// Cancela o download em andamento, se houver.
pub(crate) fn cancel_download(ui: &Ui) {
    if let Some(job) = ui.job.borrow().as_ref() {
        ui.cancel_button.set_sensitive(false);
        ui.status_label.set_text(&gettext("Cancelling…"));
        job.cancel();
    }
}

/// Mostra o último arquivo concluído no gerenciador de arquivos.
pub(crate) fn show_last_file(ui: &Ui) {
    let Some(path) = ui.session.borrow().last_file.clone() else {
        return;
    };
    let parent = ui.window.is_visible().then_some(&ui.window);
    gtk::FileLauncher::new(Some(&gio::File::for_path(path))).open_containing_folder(
        parent,
        gio::Cancellable::NONE,
        |_| {},
    );
}

/// Indica se há alguma janela do aplicativo na tela.
fn on_screen(ui: &Ui) -> bool {
    ui.window.is_visible() || details::is_visible(ui)
}

/// Encerra o aplicativo se não sobrou janela, bandeja nem download: nesse
/// estado ele ficaria rodando sem nenhuma forma de ser alcançado.
pub(crate) fn quit_if_unattended(ui: &Ui) {
    if !on_screen(ui) && ui.tray.borrow().is_none() && ui.job.borrow().is_none() {
        destroy_windows(ui);
    }
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
    if busy {
        // Um campo desativado com o foco nunca recebe a perda de foco.
        release_focus(ui);
    }
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
    let format = match kind {
        Kind::Video => format!("MP4 · {}", quality_label(max_height)),
        Kind::Audio => "MP3".to_string(),
    };
    let folder = display_folder(&request.folder);

    let (handle, events) = match downloader::start(request) {
        Ok(started) => started,
        Err(err) => {
            show_error(ui, &gettext("Could not start yt-dlp"), &err.to_string());
            return;
        }
    };

    ui.job.replace(Some(handle));
    ui.session.borrow_mut().begin(format, folder);
    ui.tray_flash.set(false);
    details::clear_thumbnail(ui);
    ui.pulsing.set(false);
    ui.title_label.set_text(&gettext("Fetching information…"));
    ui.status_label.set_text(&gettext("Connecting…"));
    ui.progress_bar.set_fraction(0.0);
    ui.progress_box.set_visible(true);
    ui.chart.queue_draw();
    set_busy(ui, true);
    start_pulse(ui);
    refresh_views(ui, true);

    let ui = ui.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            handle_event(&ui, event);
        }
    });
}

/// Atualiza o painel de detalhes, o gráfico da janela e a bandeja.
fn refresh_views(ui: &Ui, force: bool) {
    ui.chart.queue_draw();
    details::refresh(ui);
    refresh_tray(ui, force);
}

fn handle_event(ui: &Rc<Ui>, event: Event) {
    match event {
        Event::Info(info) => {
            ui.title_label.set_text(&info.title);
            let thumbnail = info.thumbnail.clone();
            ui.session.borrow_mut().set_info(info);
            details::clear_thumbnail(ui);
            if let Some(url) = thumbnail {
                details::load_thumbnail(ui, url);
            }
            refresh_views(ui, true);
        }
        Event::Progress(progress) => {
            ui.pulsing.set(false);
            ui.progress_bar.set_fraction(progress.fraction);

            let percent = format!("{:.0}", (progress.fraction * 100.0).floor());
            let mut parts = vec![
                fill(
                    &gettext("Downloading — {percent}%"),
                    &[("percent", &percent)],
                ),
                stats::size_text(&progress),
            ];
            let rate = stats::rate_text(&progress);
            if !rate.is_empty() {
                parts.push(rate);
            }
            ui.status_label.set_text(&parts.join(" · "));

            ui.session.borrow_mut().record(progress);
            refresh_views(ui, false);
        }
        Event::PostProcessing => {
            ui.status_label
                .set_text(&gettext("Processing with FFmpeg…"));
            start_pulse(ui);
            ui.session.borrow_mut().phase = Phase::Processing;
            refresh_views(ui, true);
        }
        Event::File(path) => {
            ui.session.borrow_mut().last_file = Some(path);
        }
        Event::Finished(outcome) => finish(ui, outcome),
    }
}

fn finish(ui: &Rc<Ui>, outcome: Outcome) {
    ui.job.replace(None);
    ui.pulsing.set(false);
    set_busy(ui, false);
    refresh_deps(ui);

    ui.session.borrow_mut().finish(match outcome {
        Outcome::Done => Phase::Done,
        Outcome::Cancelled => Phase::Cancelled,
        Outcome::Failed(_) => Phase::Failed,
    });
    // O ícone da bandeja mostra o resultado por alguns segundos.
    if outcome != Outcome::Cancelled {
        ui.tray_flash.set(true);
        glib::timeout_add_local_once(
            Duration::from_secs(6),
            clone!(
                #[weak]
                ui,
                move || {
                    if !ui.session.borrow().phase.is_active() {
                        ui.tray_flash.set(false);
                        refresh_tray(&ui, true);
                    }
                }
            ),
        );
    }
    refresh_views(ui, true);

    // Sem nenhuma janela na tela ou com a principal fora de foco, o
    // resultado vai também para uma notificação do sistema.
    let hidden = !on_screen(ui);
    let unattended = !ui.window.is_visible() || !ui.window.is_active();
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
            if ui.session.borrow().last_file.is_some() {
                done.set_button_label(Some(&gettext("Show in folder")));
                done.connect_button_clicked(clone!(
                    #[weak]
                    ui,
                    move |_| show_last_file(&ui)
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
            if ui.window.is_visible() {
                show_error(ui, &gettext("The download failed"), &message);
            }
        }
    }

    // Sem janela e sem bandeja, não há mais o que fazer em segundo plano. A
    // pequena espera dá tempo de a notificação ser entregue.
    if hidden {
        glib::timeout_add_local_once(
            Duration::from_millis(500),
            clone!(
                #[weak]
                ui,
                move || quit_if_unattended(&ui)
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
