//! Painel de detalhes do download: miniatura, anel de progresso, gráfico de
//! velocidade e números. É uma janela própria, para poder ficar aberta
//! enquanto a principal está escondida em segundo plano.

use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib::clone;
use gtk::{gdk, gio, glib};

use crate::charts;
use crate::stats::{self, Phase};
use crate::window::{self, Ui};

pub(crate) struct Details {
    pub(crate) window: adw::Window,
    picture: gtk::Picture,
    title: gtk::Label,
    activity: gtk::Label,
    ring: gtk::DrawingArea,
    size: gtk::Label,
    rate: gtk::Label,
    chart: gtk::DrawingArea,
    average: adw::ActionRow,
    peak: adw::ActionRow,
    elapsed: adw::ActionRow,
    format: adw::ActionRow,
    folder: adw::ActionRow,
    playlist: adw::ActionRow,
    cancel: gtk::Button,
    show_file: gtk::Button,
}

fn property_row(title: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle("—")
        .subtitle_selectable(true)
        .css_classes(["property"])
        .build()
}

fn build(ui: &Rc<Ui>) -> Details {
    // --- Cabeçalho: miniatura e título -----------------------------------
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .width_request(144)
        .height_request(81)
        .valign(gtk::Align::Start)
        .overflow(gtk::Overflow::Hidden)
        .css_classes(["card"])
        .build();
    picture.update_property(&[gtk::accessible::Property::Label(&gettext("Thumbnail"))]);

    let title = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .lines(3)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["title-4"])
        .build();
    let activity = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    let heading = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    heading.append(&title);
    heading.append(&activity);

    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    hero.append(&picture);
    hero.append(&heading);

    // --- Progresso: anel e números principais ----------------------------
    let ring = charts::progress_ring(&ui.session, 92);
    let size = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["title-2", "numeric"])
        .build();
    let rate = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["dim-label", "numeric"])
        .build();
    let numbers = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();
    numbers.append(&size);
    numbers.append(&rate);

    let progress = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    progress.append(&ring);
    progress.append(&numbers);

    // --- Gráfico ---------------------------------------------------------
    let chart_title = gtk::Label::builder()
        .label(gettext("Download speed"))
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let chart = charts::speed_chart(&ui.session, 170);
    let chart_card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .css_classes(["card"])
        .build();
    let chart_inner = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    chart_inner.append(&chart_title);
    chart_inner.append(&chart);
    chart_card.append(&chart_inner);

    // --- Números em lista (também servem de alternativa ao gráfico) -------
    let average = property_row(&gettext("Average speed"));
    let peak = property_row(&gettext("Peak speed"));
    let elapsed = property_row(&gettext("Elapsed time"));
    let format = property_row(&gettext("Format"));
    let folder = property_row(&gettext("Destination folder"));
    let playlist = property_row(&gettext("Playlist"));
    let list = adw::PreferencesGroup::new();
    for row in [&average, &peak, &elapsed, &format, &folder, &playlist] {
        list.add(row);
    }

    // --- Botões ----------------------------------------------------------
    let cancel = gtk::Button::builder()
        .label(gettext("Cancel download"))
        .css_classes(["pill", "destructive-action"])
        .build();
    let show_file = gtk::Button::builder()
        .label(gettext("Show in folder"))
        .css_classes(["pill"])
        .build();
    let buttons = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .build();
    buttons.append(&cancel);
    buttons.append(&show_file);

    // --- Layout ----------------------------------------------------------
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(20)
        .margin_top(18)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&hero);
    content.append(&progress);
    content.append(&chart_card);
    content.append(&list);
    content.append(&buttons);

    let clamp = adw::Clamp::builder()
        .maximum_size(520)
        .child(&content)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&adw::HeaderBar::new());
    toolbar_view.set_content(Some(&scrolled));

    let window = adw::Window::builder()
        .title(gettext("Download details"))
        .icon_name(crate::config::APP_ID)
        .default_width(440)
        .default_height(680)
        .hide_on_close(true)
        .content(&toolbar_view)
        .build();
    if let Some(app) = ui.window.application() {
        window.set_application(Some(&app));
    }

    cancel.connect_clicked(clone!(
        #[weak]
        ui,
        move |_| window::cancel_download(&ui)
    ));
    show_file.connect_clicked(clone!(
        #[weak]
        ui,
        move |_| window::show_last_file(&ui)
    ));
    // Fechar o painel com a janela principal escondida e nada em andamento
    // deixaria o aplicativo rodando sem nenhuma janela.
    window.connect_close_request(clone!(
        #[weak]
        ui,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_| {
            glib::idle_add_local_once(clone!(
                #[weak]
                ui,
                move || window::quit_if_unattended(&ui)
            ));
            glib::Propagation::Proceed
        }
    ));
    // O gráfico e o anel usam cores que mudam com o tema.
    adw::StyleManager::default().connect_dark_notify(clone!(
        #[weak]
        ring,
        #[weak]
        chart,
        move |_| {
            ring.queue_draw();
            chart.queue_draw();
        }
    ));

    Details {
        window,
        picture,
        title,
        activity,
        ring,
        size,
        rate,
        chart,
        average,
        peak,
        elapsed,
        format,
        folder,
        playlist,
        cancel,
        show_file,
    }
}

/// Abre o painel (criando-o na primeira vez) e o mantém atualizado.
pub(crate) fn show(ui: &Rc<Ui>) {
    if ui.details.borrow().is_none() {
        let details = build(ui);
        ui.details.replace(Some(details));

        // O tempo decorrido anda mesmo sem novos dados do yt-dlp.
        glib::timeout_add_seconds_local(
            1,
            clone!(
                #[weak]
                ui,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    if ui.details.borrow().is_none() {
                        return glib::ControlFlow::Break;
                    }
                    if ui.session.borrow().phase.is_active() {
                        refresh(&ui);
                    }
                    glib::ControlFlow::Continue
                }
            ),
        );
    }
    refresh(ui);
    if let Some(details) = ui.details.borrow().as_ref() {
        details
            .picture
            .set_paintable(ui.thumbnail.borrow().as_ref());
        details.window.present();
    }
}

/// Indica se o painel está aberto na tela.
pub(crate) fn is_visible(ui: &Ui) -> bool {
    ui.details
        .borrow()
        .as_ref()
        .is_some_and(|details| details.window.is_visible())
}

/// Atualiza os textos e redesenha os gráficos, se o painel existir.
pub(crate) fn refresh(ui: &Ui) {
    let details = ui.details.borrow();
    let Some(details) = details.as_ref() else {
        return;
    };
    if !details.window.is_visible() && details.window.is_realized() {
        // Escondido: será atualizado ao reabrir.
        return;
    }
    let session = ui.session.borrow();

    let title = if session.info.title.is_empty() {
        match session.phase {
            Phase::Idle => gettext("No download in progress"),
            _ => gettext("Fetching information…"),
        }
    } else {
        session.info.title.clone()
    };
    details.title.set_text(&title);

    let mut activity = session.activity();
    if let Some(playlist) = session.playlist_text() {
        activity = format!("{activity} · {playlist}");
    }
    details.activity.set_text(&activity);
    details.activity.set_visible(!session.info.title.is_empty());

    match session.progress.as_ref() {
        Some(progress) => {
            details.size.set_text(&stats::size_text(progress));
            let rate = if session.phase == Phase::Downloading {
                stats::rate_text(progress)
            } else {
                String::new()
            };
            details.rate.set_text(&rate);
            details.rate.set_visible(!rate.is_empty());
        }
        None => {
            details.size.set_text("—");
            details.rate.set_visible(false);
        }
    }

    let or_dash = |value: Option<String>| value.unwrap_or_else(|| "—".to_string());
    details
        .average
        .set_subtitle(&or_dash(session.average_speed().map(stats::format_speed)));
    details
        .peak
        .set_subtitle(&or_dash(session.peak_speed().map(stats::format_speed)));
    details.elapsed.set_subtitle(&or_dash(
        session
            .elapsed()
            .map(|seconds| stats::format_duration(seconds as u64)),
    ));
    details.format.set_subtitle(&or_dash(
        Some(session.format.clone()).filter(|f| !f.is_empty()),
    ));
    details.folder.set_subtitle(&or_dash(
        Some(session.folder.clone()).filter(|f| !f.is_empty()),
    ));
    let playlist = session.playlist_text();
    details.playlist.set_visible(playlist.is_some());
    details.playlist.set_subtitle(&or_dash(playlist));

    details.cancel.set_visible(session.phase.is_active());
    details.show_file.set_visible(session.last_file.is_some());

    details.ring.queue_draw();
    details.chart.queue_draw();
}

/// Baixa a miniatura do vídeo e a mostra no painel. Falhas são ignoradas:
/// a miniatura é só um enfeite.
pub(crate) fn load_thumbnail(ui: &Rc<Ui>, url: String) {
    glib::spawn_future_local(clone!(
        #[weak]
        ui,
        async move {
            let file = gio::File::for_uri(&url);
            let Ok((bytes, _)) = file.load_bytes_future().await else {
                return;
            };
            let Ok(texture) = gdk::Texture::from_bytes(&bytes) else {
                return;
            };
            // O item pode ter mudado enquanto a imagem era baixada.
            if ui.session.borrow().info.thumbnail.as_deref() != Some(url.as_str()) {
                return;
            }
            ui.thumbnail.replace(Some(texture));
            if let Some(details) = ui.details.borrow().as_ref() {
                details
                    .picture
                    .set_paintable(ui.thumbnail.borrow().as_ref());
            }
        }
    ));
}

/// Remove a miniatura (novo download ou item sem imagem).
pub(crate) fn clear_thumbnail(ui: &Ui) {
    ui.thumbnail.replace(None);
    if let Some(details) = ui.details.borrow().as_ref() {
        details.picture.set_paintable(gdk::Paintable::NONE);
    }
}
