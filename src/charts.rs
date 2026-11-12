//! Desenhos feitos com cairo: gráfico de velocidade, anel de progresso e o
//! ícone da bandeja.
//!
//! Cores: vídeo e áudio usam os dois primeiros tons de uma paleta
//! categórica validada para daltonismo e contraste, com um passo próprio
//! para o tema escuro. Verde e vermelho ficam reservados para estado
//! (concluído / falhou). Textos usam sempre a cor de texto do tema, nunca a
//! cor da série.

use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::cairo;

use crate::downloader::Stage;
use crate::stats::{self, Phase, Sample, Session};

type Rgb = (f64, f64, f64);

const fn rgb(hex: u32) -> Rgb {
    (
        ((hex >> 16) & 0xff) as f64 / 255.0,
        ((hex >> 8) & 0xff) as f64 / 255.0,
        (hex & 0xff) as f64 / 255.0,
    )
}

const VIDEO_LIGHT: Rgb = rgb(0x2a78d6);
const VIDEO_DARK: Rgb = rgb(0x3987e5);
const AUDIO_LIGHT: Rgb = rgb(0xeb6834);
const AUDIO_DARK: Rgb = rgb(0xd95926);
const GOOD: Rgb = rgb(0x0ca30c);
const CRITICAL: Rgb = rgb(0xd03b3b);

const FONT: &str = "sans-serif";

/// Cor da série de cada etapa. Um arquivo único (vídeo + áudio) usa a
/// primeira cor.
fn series_color(stage: Stage, dark: bool) -> Rgb {
    match (stage, dark) {
        (Stage::Audio, false) => AUDIO_LIGHT,
        (Stage::Audio, true) => AUDIO_DARK,
        (_, false) => VIDEO_LIGHT,
        (_, true) => VIDEO_DARK,
    }
}

/// Cores que dependem do tema.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub dark: bool,
    /// Cor de texto do tema.
    pub ink: Rgb,
}

impl Style {
    pub fn for_widget(widget: &impl IsA<gtk::Widget>) -> Self {
        let color = widget.color();
        Self {
            dark: adw::StyleManager::default().is_dark(),
            ink: (
                color.red() as f64,
                color.green() as f64,
                color.blue() as f64,
            ),
        }
    }

    fn ink(&self, cr: &cairo::Context, alpha: f64) {
        cr.set_source_rgba(self.ink.0, self.ink.1, self.ink.2, alpha);
    }

    /// Fundo aproximado do tema, para caixas sobre o gráfico.
    fn surface(&self) -> Rgb {
        if self.dark {
            rgb(0x2e2e32)
        } else {
            rgb(0xffffff)
        }
    }
}

fn set_rgb(cr: &cairo::Context, color: Rgb, alpha: f64) {
    cr.set_source_rgba(color.0, color.1, color.2, alpha);
}

fn set_font(cr: &cairo::Context, size: f64, bold: bool) {
    let weight = if bold {
        cairo::FontWeight::Bold
    } else {
        cairo::FontWeight::Normal
    };
    cr.select_font_face(FONT, cairo::FontSlant::Normal, weight);
    cr.set_font_size(size);
}

fn text_width(cr: &cairo::Context, text: &str) -> f64 {
    cr.text_extents(text)
        .map(|extents| extents.x_advance())
        .unwrap_or(0.0)
}

fn show_text(cr: &cairo::Context, x: f64, y: f64, text: &str) {
    cr.move_to(x, y);
    let _ = cr.show_text(text);
    // O texto deixa um ponto corrente; sem limpá-lo, o próximo arco
    // nasceria ligado a ele por uma linha.
    cr.new_path();
}

fn rounded_rect(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        PI / 2.0,
    );
    cr.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
    cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    cr.close_path();
}

/// Menor número "redondo" (1, 2 ou 5 vezes uma potência de dez) que não
/// fica abaixo do valor; usado como topo do eixo.
fn nice_ceiling(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return 1.0;
    }
    let power = 10f64.powf(value.log10().floor());
    let fraction = value / power;
    let nice = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|step| fraction <= *step + 1e-9)
        .unwrap_or(10.0);
    nice * power
}

/// Textos do gráfico, já traduzidos.
pub struct ChartText {
    pub waiting: String,
    pub video: String,
    pub audio: String,
}

impl ChartText {
    pub fn translated() -> Self {
        Self {
            waiting: gettext("Waiting for data…"),
            video: gettext("Video"),
            audio: gettext("Audio"),
        }
    }
}

/// Gráfico de área da velocidade ao longo do tempo. Cada etapa (vídeo,
/// áudio) é uma série com a própria cor.
pub fn draw_speed_chart(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    samples: &[Sample],
    style: &Style,
    text: &ChartText,
    hover_x: Option<f64>,
) {
    const LABEL_SIZE: f64 = 11.0;

    if samples.len() < 2 {
        set_font(cr, 12.0, false);
        style.ink(cr, 0.55);
        let label_width = text_width(cr, &text.waiting);
        show_text(
            cr,
            (width - label_width) / 2.0,
            height / 2.0 + 4.0,
            &text.waiting,
        );
        return;
    }

    let has_video = samples.iter().any(|s| s.stage != Stage::Audio);
    let has_audio = samples.iter().any(|s| s.stage == Stage::Audio);
    let legend = has_video && has_audio;

    let peak = samples.iter().map(|s| s.speed).fold(0.0, f64::max);
    let top = nice_ceiling(peak);
    let ticks = [0.0, top / 2.0, top];

    set_font(cr, LABEL_SIZE, false);
    let tick_labels: Vec<String> = ticks
        .iter()
        .map(|value| {
            if *value == 0.0 {
                "0".to_string()
            } else {
                stats::format_speed(*value)
            }
        })
        .collect();
    let gutter = tick_labels
        .iter()
        .map(|label| text_width(cr, label))
        .fold(0.0, f64::max)
        + 8.0;

    let plot_left = gutter;
    let plot_right = width - 8.0;
    let plot_top = if legend { 26.0 } else { 10.0 };
    let plot_bottom = height - 6.0;
    let plot_width = (plot_right - plot_left).max(1.0);
    let plot_height = (plot_bottom - plot_top).max(1.0);

    let start = samples[0].time;
    // Janela mínima de alguns segundos, para o início não parecer esticado.
    let span = (samples[samples.len() - 1].time - start).max(8.0);
    let x_of = |time: f64| plot_left + (time - start) / span * plot_width;
    let y_of = |speed: f64| plot_bottom - (speed / top).clamp(0.0, 1.0) * plot_height;

    // Grade e rótulos do eixo: discretos, na cor de texto esmaecida.
    cr.set_line_width(1.0);
    for (value, label) in ticks.iter().zip(&tick_labels) {
        let y = y_of(*value).round() + 0.5;
        style.ink(cr, if *value == 0.0 { 0.25 } else { 0.12 });
        cr.move_to(plot_left, y);
        cr.line_to(plot_right, y);
        let _ = cr.stroke();

        style.ink(cr, 0.55);
        let label_width = text_width(cr, label);
        show_text(cr, plot_left - 8.0 - label_width, y + 4.0, label);
    }

    // Séries: um trecho por etapa, com a divisa compartilhada.
    let mut runs: Vec<(Stage, Vec<&Sample>)> = Vec::new();
    for sample in samples {
        match runs.last_mut() {
            Some((stage, run)) if *stage == sample.stage => run.push(sample),
            _ => {
                let mut run = Vec::new();
                if let Some((_, previous)) = runs.last() {
                    run.push(previous[previous.len() - 1]);
                }
                run.push(sample);
                runs.push((sample.stage, run));
            }
        }
    }

    cr.set_line_join(cairo::LineJoin::Round);
    cr.set_line_cap(cairo::LineCap::Round);
    for (stage, run) in &runs {
        let color = series_color(*stage, style.dark);

        // Área: a cor da série bem suave.
        cr.move_to(x_of(run[0].time), plot_bottom);
        for sample in run {
            cr.line_to(x_of(sample.time), y_of(sample.speed));
        }
        cr.line_to(x_of(run[run.len() - 1].time), plot_bottom);
        cr.close_path();
        set_rgb(cr, color, 0.14);
        let _ = cr.fill();

        cr.move_to(x_of(run[0].time), y_of(run[0].speed));
        for sample in &run[1..] {
            cr.line_to(x_of(sample.time), y_of(sample.speed));
        }
        set_rgb(cr, color, 1.0);
        cr.set_line_width(2.0);
        let _ = cr.stroke();
    }

    // Legenda: só quando há duas séries.
    if legend {
        set_font(cr, LABEL_SIZE, false);
        let mut x = plot_left;
        for (stage, label) in [(Stage::Video, &text.video), (Stage::Audio, &text.audio)] {
            rounded_rect(cr, x, 6.0, 10.0, 10.0, 3.0);
            set_rgb(cr, series_color(stage, style.dark), 1.0);
            let _ = cr.fill();
            style.ink(cr, 0.75);
            show_text(cr, x + 15.0, 15.0, label);
            x += 15.0 + text_width(cr, label) + 16.0;
        }
    }

    // Ponto em destaque: o mais próximo do cursor ou, sem cursor, o último.
    let last = &samples[samples.len() - 1];
    let focus = hover_x
        .filter(|x| *x >= plot_left - 4.0 && *x <= plot_right + 4.0)
        .and_then(|x| {
            samples.iter().min_by(|a, b| {
                (x_of(a.time) - x)
                    .abs()
                    .total_cmp(&(x_of(b.time) - x).abs())
            })
        });
    let hovering = focus.is_some();
    let point = focus.unwrap_or(last);
    let (px, py) = (x_of(point.time), y_of(point.speed));

    if hovering {
        style.ink(cr, 0.3);
        cr.set_line_width(1.0);
        cr.move_to(px.round() + 0.5, plot_top);
        cr.line_to(px.round() + 0.5, plot_bottom);
        let _ = cr.stroke();
    }

    // Anel na cor do fundo, para o ponto não se confundir com a linha.
    set_rgb(cr, style.surface(), 1.0);
    cr.arc(px, py, 6.0, 0.0, 2.0 * PI);
    let _ = cr.fill();
    set_rgb(cr, series_color(point.stage, style.dark), 1.0);
    cr.arc(px, py, 4.0, 0.0, 2.0 * PI);
    let _ = cr.fill();

    let label = if hovering {
        format!(
            "{} · {}",
            stats::format_speed(point.speed),
            stats::format_duration((point.time - start).max(0.0) as u64)
        )
    } else {
        stats::format_speed(point.speed)
    };
    set_font(cr, LABEL_SIZE, true);
    let label_width = text_width(cr, &label);
    let box_width = label_width + 12.0;
    let box_height = 20.0;
    // Só é desenhado se couber; prefere ficar acima e à esquerda do ponto.
    if box_width <= plot_width {
        let mut box_x = px - box_width - 8.0;
        if box_x < plot_left {
            box_x = (px + 8.0).min(plot_right - box_width);
        }
        let mut box_y = py - box_height - 8.0;
        if box_y < 2.0 {
            box_y = py + 8.0;
        }
        rounded_rect(cr, box_x, box_y, box_width, box_height, 6.0);
        set_rgb(cr, style.surface(), 0.92);
        let _ = cr.fill_preserve();
        style.ink(cr, 0.15);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        style.ink(cr, 0.9);
        show_text(cr, box_x + 6.0, box_y + 14.0, &label);
    }
}

/// O que o anel de progresso representa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ring {
    /// Em andamento, com a fração concluída (0 a 1).
    Progress(f64),
    /// Em andamento, sem porcentagem conhecida.
    Busy,
    Done,
    Failed,
    Idle,
}

impl Ring {
    pub fn for_session(session: &Session) -> Self {
        match session.phase {
            Phase::Idle | Phase::Cancelled => Ring::Idle,
            Phase::Starting => Ring::Busy,
            Phase::Downloading => session
                .progress
                .as_ref()
                .map_or(Ring::Busy, |progress| Ring::Progress(progress.fraction)),
            Phase::Processing => Ring::Progress(1.0),
            Phase::Done => Ring::Done,
            Phase::Failed => Ring::Failed,
        }
    }
}

fn draw_check(cr: &cairo::Context, center: f64, size: f64, line_width: f64) {
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    cr.move_to(center - size * 0.42, center + size * 0.02);
    cr.line_to(center - size * 0.1, center + size * 0.34);
    cr.line_to(center + size * 0.46, center - size * 0.3);
    let _ = cr.stroke();
}

fn draw_exclamation(cr: &cairo::Context, center: f64, size: f64, line_width: f64) {
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.move_to(center, center - size * 0.42);
    cr.line_to(center, center + size * 0.08);
    let _ = cr.stroke();
    cr.arc(center, center + size * 0.4, line_width * 0.6, 0.0, 2.0 * PI);
    let _ = cr.fill();
}

/// Anel de progresso quadrado de lado `size`, com a porcentagem no centro.
pub fn draw_ring(cr: &cairo::Context, size: f64, ring: Ring, style: &Style) {
    let thickness = (size * 0.09).max(3.0);
    let center = size / 2.0;
    let radius = center - thickness / 2.0 - 1.0;
    let inner = radius - thickness;

    cr.set_line_width(thickness);
    cr.set_line_cap(cairo::LineCap::Round);
    style.ink(cr, 0.12);
    cr.arc(center, center, radius, 0.0, 2.0 * PI);
    let _ = cr.stroke();

    let (fraction, color) = match ring {
        Ring::Progress(fraction) => (
            fraction.clamp(0.0, 1.0),
            series_color(Stage::Video, style.dark),
        ),
        Ring::Done => (1.0, GOOD),
        Ring::Failed => (1.0, CRITICAL),
        Ring::Busy | Ring::Idle => (0.0, series_color(Stage::Video, style.dark)),
    };
    if fraction > 0.0 {
        set_rgb(cr, color, 1.0);
        cr.arc(
            center,
            center,
            radius,
            -PI / 2.0,
            -PI / 2.0 + 2.0 * PI * fraction,
        );
        let _ = cr.stroke();
    }

    // O centro usa a cor de texto; a cor fica só no anel.
    style.ink(cr, 0.9);
    match ring {
        Ring::Progress(fraction) => {
            let label = format!("{}%", (fraction * 100.0).floor().clamp(0.0, 100.0) as u8);
            set_font(cr, size * 0.24, true);
            if let Ok(extents) = cr.text_extents(&label) {
                show_text(
                    cr,
                    center - extents.width() / 2.0 - extents.x_bearing(),
                    center - extents.height() / 2.0 - extents.y_bearing(),
                    &label,
                );
            }
        }
        Ring::Done => draw_check(cr, center, inner * 0.8, thickness * 0.8),
        Ring::Failed => draw_exclamation(cr, center, inner * 0.8, thickness * 0.8),
        Ring::Busy => {
            style.ink(cr, 0.55);
            for offset in [-1.0, 0.0, 1.0] {
                cr.arc(
                    center + offset * size * 0.12,
                    center,
                    size * 0.03,
                    0.0,
                    2.0 * PI,
                );
                let _ = cr.fill();
            }
        }
        Ring::Idle => {}
    }
}

/// Estado mostrado no ícone da bandeja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIcon {
    /// Download em andamento, com a porcentagem (0 a 100).
    Progress(u8),
    /// Em andamento, sem porcentagem conhecida.
    Busy,
    Done,
    Failed,
}

/// Imagem no formato do StatusNotifierItem: ARGB32, um byte por canal na
/// ordem A, R, G, B, sem alfa pré-multiplicado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixmap {
    pub size: i32,
    pub argb: Vec<u8>,
}

/// Tamanhos gerados; a bandeja escolhe o mais adequado à tela.
const TRAY_SIZES: [i32; 3] = [24, 48, 96];

fn tray_surface(icon: TrayIcon, size: i32) -> Option<cairo::ImageSurface> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let size = size as f64;
        let center = size / 2.0;
        let thickness = size * 0.12;
        let radius = center - thickness / 2.0 - size * 0.02;

        // A barra superior do GNOME é sempre escura: branco para o que é
        // texto, e os passos de tema escuro para as cores.
        let white = (1.0, 1.0, 1.0);
        let (fraction, color) = match icon {
            TrayIcon::Progress(percent) => (percent.min(100) as f64 / 100.0, VIDEO_DARK),
            TrayIcon::Busy => (0.0, VIDEO_DARK),
            TrayIcon::Done => (1.0, GOOD),
            TrayIcon::Failed => (1.0, CRITICAL),
        };

        cr.set_line_width(thickness);
        cr.set_line_cap(cairo::LineCap::Round);
        set_rgb(&cr, white, 0.3);
        cr.arc(center, center, radius, 0.0, 2.0 * PI);
        let _ = cr.stroke();
        if fraction > 0.0 {
            set_rgb(&cr, color, 1.0);
            cr.arc(
                center,
                center,
                radius,
                -PI / 2.0,
                -PI / 2.0 + 2.0 * PI * fraction,
            );
            let _ = cr.stroke();
        }

        set_rgb(&cr, white, 1.0);
        let inner = radius - thickness / 2.0;
        match icon {
            TrayIcon::Progress(percent) => {
                let label = percent.min(100).to_string();
                // Dois dígitos ocupam o miolo inteiro; três precisam encolher.
                let scale = if label.len() >= 3 { 0.74 } else { 1.02 };
                set_font(&cr, inner * scale, true);
                if let Ok(extents) = cr.text_extents(&label) {
                    show_text(
                        &cr,
                        center - extents.width() / 2.0 - extents.x_bearing(),
                        center - extents.height() / 2.0 - extents.y_bearing(),
                        &label,
                    );
                }
            }
            TrayIcon::Busy => {
                for offset in [-1.0, 0.0, 1.0] {
                    cr.arc(
                        center + offset * inner * 0.5,
                        center,
                        inner * 0.14,
                        0.0,
                        2.0 * PI,
                    );
                    let _ = cr.fill();
                }
            }
            TrayIcon::Done => draw_check(&cr, center, inner * 0.9, thickness),
            TrayIcon::Failed => draw_exclamation(&cr, center, inner * 0.9, thickness),
        }
    }
    surface.flush();
    Some(surface)
}

/// Converte uma superfície do cairo (ARGB pré-multiplicado, na ordem de
/// bytes da máquina) para o formato do StatusNotifierItem.
fn to_pixmap(mut surface: cairo::ImageSurface) -> Option<Pixmap> {
    let size = surface.width();
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size as usize {
        for column in 0..size as usize {
            let offset = row * stride + column * 4;
            let pixel = u32::from_ne_bytes(data[offset..offset + 4].try_into().ok()?);
            let alpha = (pixel >> 24) & 0xff;
            // Desfaz o alfa pré-multiplicado; pixel transparente vira zero.
            let straight = |channel: u32| -> u8 {
                (channel * 255 + alpha / 2)
                    .checked_div(alpha)
                    .map_or(0, |value| value.min(255) as u8)
            };
            argb.extend_from_slice(&[
                alpha as u8,
                straight((pixel >> 16) & 0xff),
                straight((pixel >> 8) & 0xff),
                straight(pixel & 0xff),
            ]);
        }
    }
    Some(Pixmap { size, argb })
}

/// Ícone da bandeja desenhado na hora, em vários tamanhos.
pub fn tray_icon(icon: TrayIcon) -> Vec<Pixmap> {
    TRAY_SIZES
        .into_iter()
        .filter_map(|size| tray_surface(icon, size).and_then(to_pixmap))
        .collect()
}

/// Miniatura do gráfico de velocidade em PNG, usada como ícone de uma linha
/// do menu da bandeja. O menu do GNOME Shell é sempre escuro.
pub fn sparkline_png(samples: &[Sample]) -> Vec<u8> {
    const SIZE: i32 = 64;
    let mut png = Vec::new();
    if samples.len() < 2 {
        return png;
    }
    let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, SIZE, SIZE) else {
        return png;
    };
    {
        let Ok(cr) = cairo::Context::new(&surface) else {
            return png;
        };
        let size = SIZE as f64;
        let pad = 5.0;
        let peak = samples.iter().map(|s| s.speed).fold(0.0, f64::max).max(1.0);
        let start = samples[0].time;
        let span = (samples[samples.len() - 1].time - start).max(1.0);
        let x_of = |time: f64| pad + (time - start) / span * (size - 2.0 * pad);
        let y_of = |speed: f64| size - pad - (speed / peak) * (size - 2.0 * pad);

        let color = series_color(samples[samples.len() - 1].stage, true);
        cr.move_to(x_of(samples[0].time), size - pad);
        for sample in samples {
            cr.line_to(x_of(sample.time), y_of(sample.speed));
        }
        cr.line_to(x_of(samples[samples.len() - 1].time), size - pad);
        cr.close_path();
        set_rgb(&cr, color, 0.3);
        let _ = cr.fill();

        cr.set_line_join(cairo::LineJoin::Round);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_width(5.0);
        cr.move_to(x_of(samples[0].time), y_of(samples[0].speed));
        for sample in &samples[1..] {
            cr.line_to(x_of(sample.time), y_of(sample.speed));
        }
        set_rgb(&cr, color, 1.0);
        let _ = cr.stroke();
    }
    let _ = surface.write_to_png(&mut png);
    png
}

/// Área de desenho com o gráfico de velocidade do download atual. Passar o
/// cursor mostra o valor de cada ponto.
pub fn speed_chart(session: &Rc<RefCell<Session>>, height: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_height(height)
        .hexpand(true)
        .build();
    area.update_property(&[gtk::accessible::Property::Label(&gettext(
        "Download speed over time",
    ))]);

    let hover: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let text = ChartText::translated();
    area.set_draw_func({
        let session = session.clone();
        let hover = hover.clone();
        move |area, cr, width, height| {
            draw_speed_chart(
                cr,
                width as f64,
                height as f64,
                &session.borrow().samples,
                &Style::for_widget(area),
                &text,
                hover.get(),
            );
        }
    });

    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion({
        let hover = hover.clone();
        let area = area.downgrade();
        move |_, x, _| {
            hover.set(Some(x));
            if let Some(area) = area.upgrade() {
                area.queue_draw();
            }
        }
    });
    motion.connect_leave({
        let area = area.downgrade();
        move |_| {
            hover.set(None);
            if let Some(area) = area.upgrade() {
                area.queue_draw();
            }
        }
    });
    area.add_controller(motion);
    area
}

/// Área de desenho com o anel de progresso do download atual.
pub fn progress_ring(session: &Rc<RefCell<Session>>, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .valign(gtk::Align::Center)
        .build();
    area.set_draw_func({
        let session = session.clone();
        move |area, cr, width, height| {
            draw_ring(
                cr,
                width.min(height) as f64,
                Ring::for_session(&session.borrow()),
                &Style::for_widget(area),
            );
        }
    });
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<Sample> {
        // Vídeo acelerando e oscilando, depois um trecho de áudio.
        (0..60)
            .map(|index| {
                let time = index as f64 * 0.5;
                let wave = (time * 0.9).sin() * 0.6e6 + (time * 2.3).cos() * 0.25e6;
                let ramp = (time / 6.0).min(1.0) * 3.2e6;
                Sample {
                    time,
                    speed: (ramp + wave).max(0.2e6),
                    stage: if index < 44 {
                        Stage::Video
                    } else {
                        Stage::Audio
                    },
                }
            })
            .collect()
    }

    #[test]
    fn axis_top_is_a_round_number() {
        assert_eq!(nice_ceiling(0.0), 1.0);
        assert_eq!(nice_ceiling(1.0), 1.0);
        assert_eq!(nice_ceiling(1.2e6), 2.0e6);
        assert_eq!(nice_ceiling(3.4e6), 5.0e6);
        assert_eq!(nice_ceiling(7.0e6), 1.0e7);
        assert_eq!(nice_ceiling(5.0e3), 5.0e3);
    }

    #[test]
    fn tray_icon_has_all_sizes_in_straight_argb() {
        let pixmaps = tray_icon(TrayIcon::Progress(62));
        let sizes: Vec<i32> = pixmaps.iter().map(|p| p.size).collect();
        assert_eq!(sizes, TRAY_SIZES);
        for pixmap in &pixmaps {
            assert_eq!(pixmap.argb.len(), (pixmap.size * pixmap.size * 4) as usize);
        }
        // O canto fica fora do anel: totalmente transparente.
        assert_eq!(&pixmaps[1].argb[..4], [0, 0, 0, 0]);
        // O centro tem o número, em branco opaco.
        let big = &pixmaps[2];
        let opaque_white = big
            .argb
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| **pixel == [255, 255, 255, 255])
            .count();
        assert!(opaque_white > 50, "brancos opacos: {opaque_white}");
    }

    #[test]
    fn each_tray_state_draws_something_different() {
        let states = [
            TrayIcon::Progress(5),
            TrayIcon::Progress(62),
            TrayIcon::Progress(100),
            TrayIcon::Busy,
            TrayIcon::Done,
            TrayIcon::Failed,
        ];
        let images: Vec<Vec<u8>> = states
            .iter()
            .map(|state| tray_icon(*state).remove(1).argb)
            .collect();
        for (index, image) in images.iter().enumerate() {
            assert!(image.iter().any(|byte| *byte != 0));
            assert!(!images[..index].contains(image), "estado {index} repetido");
        }
    }

    #[test]
    fn sparkline_needs_two_points() {
        assert!(sparkline_png(&samples()[..1]).is_empty());
        let png = sparkline_png(&samples());
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    /// Gera imagens para conferência visual:
    /// CASCATA_PREVIEW_DIR=/algum/lugar cargo test render_previews -- --ignored
    #[test]
    #[ignore]
    fn render_previews() {
        let dir = std::path::PathBuf::from(
            std::env::var("CASCATA_PREVIEW_DIR").expect("defina CASCATA_PREVIEW_DIR"),
        );
        std::fs::create_dir_all(&dir).unwrap();
        let text = ChartText {
            waiting: "Waiting for data…".into(),
            video: "Video".into(),
            audio: "Audio".into(),
        };
        let save = |surface: &cairo::ImageSurface, name: &str| {
            let mut file = std::fs::File::create(dir.join(name)).unwrap();
            surface.write_to_png(&mut file).unwrap();
        };

        for (dark, name) in [(false, "light"), (true, "dark")] {
            let style = Style {
                dark,
                ink: if dark {
                    (1.0, 1.0, 1.0)
                } else {
                    (0.0, 0.0, 0.06)
                },
            };
            let background = style.surface();

            // Gráfico: completo, com cursor, só vídeo e vazio.
            let all = samples();
            let cases: [(&str, &[Sample], Option<f64>); 4] = [
                ("chart", &all, None),
                ("chart-hover", &all, Some(190.0)),
                ("chart-video", &all[..30], None),
                ("chart-empty", &all[..1], None),
            ];
            for (case, data, hover) in cases {
                let (width, height) = (420, 170);
                let surface =
                    cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).unwrap();
                let cr = cairo::Context::new(&surface).unwrap();
                set_rgb(&cr, background, 1.0);
                let _ = cr.paint();
                draw_speed_chart(&cr, width as f64, height as f64, data, &style, &text, hover);
                drop(cr);
                save(&surface, &format!("{case}-{name}.png"));
            }

            // Anéis, lado a lado.
            let rings = [
                Ring::Busy,
                Ring::Progress(0.07),
                Ring::Progress(0.62),
                Ring::Progress(1.0),
                Ring::Done,
                Ring::Failed,
            ];
            let size = 96;
            let surface = cairo::ImageSurface::create(
                cairo::Format::ARgb32,
                size * rings.len() as i32 + 16 * (rings.len() as i32 + 1),
                size + 32,
            )
            .unwrap();
            let cr = cairo::Context::new(&surface).unwrap();
            set_rgb(&cr, background, 1.0);
            let _ = cr.paint();
            for (index, ring) in rings.iter().enumerate() {
                let _ = cr.save();
                cr.translate(16.0 + index as f64 * (size + 16) as f64, 16.0);
                draw_ring(&cr, size as f64, *ring, &style);
                let _ = cr.restore();
            }
            drop(cr);
            save(&surface, &format!("rings-{name}.png"));
        }

        // Ícones da bandeja sobre a cor da barra do GNOME: ampliados e no
        // tamanho real (22 px).
        let states = [
            TrayIcon::Busy,
            TrayIcon::Progress(7),
            TrayIcon::Progress(62),
            TrayIcon::Progress(99),
            TrayIcon::Progress(100),
            TrayIcon::Done,
            TrayIcon::Failed,
        ];
        let big = 96;
        let surface = cairo::ImageSurface::create(
            cairo::Format::ARgb32,
            (big + 16) * states.len() as i32 + 16,
            big + 32 + 22 * 3 + 32,
        )
        .unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(0.075, 0.075, 0.075);
        let _ = cr.paint();
        for (index, state) in states.iter().enumerate() {
            let x = 16.0 + index as f64 * (big + 16) as f64;
            let large = tray_surface(*state, big).unwrap();
            let _ = cr.set_source_surface(&large, x, 16.0);
            let _ = cr.paint();

            // Tamanho real, ampliado 3× sem suavização para ver os pixels.
            let actual = tray_surface(*state, 48).unwrap();
            let small = cairo::ImageSurface::create(cairo::Format::ARgb32, 22, 22).unwrap();
            let small_cr = cairo::Context::new(&small).unwrap();
            small_cr.scale(22.0 / 48.0, 22.0 / 48.0);
            let _ = small_cr.set_source_surface(&actual, 0.0, 0.0);
            let _ = small_cr.paint();
            drop(small_cr);

            let _ = cr.save();
            cr.translate(x + 15.0, big as f64 + 40.0);
            cr.scale(3.0, 3.0);
            let _ = cr.set_source_surface(&small, 0.0, 0.0);
            cr.source().set_filter(cairo::Filter::Nearest);
            let _ = cr.paint();
            let _ = cr.restore();
        }
        drop(cr);
        save(&surface, "tray-icons.png");

        std::fs::write(dir.join("sparkline.png"), sparkline_png(&samples())).unwrap();
    }
}
