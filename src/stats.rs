//! Estado do download em andamento e formatação dos números mostrados na
//! janela, no painel de detalhes e na bandeja.

use std::time::Instant;

use gettextrs::gettext;
use gtk::glib;

use crate::downloader::{Info, Progress, Stage};
use crate::i18n::fill;

/// Intervalo mínimo entre duas amostras do gráfico de velocidade.
const SAMPLE_INTERVAL: f64 = 0.25;
/// Quantidade máxima de amostras guardadas.
const MAX_SAMPLES: usize = 480;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// O yt-dlp foi iniciado, mas ainda não informou nada.
    Starting,
    Downloading,
    /// Etapa do FFmpeg (junção ou conversão).
    Processing,
    Done,
    Failed,
    Cancelled,
}

impl Phase {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Phase::Starting | Phase::Downloading | Phase::Processing
        )
    }
}

/// Um ponto do gráfico de velocidade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Segundos desde o início do download.
    pub time: f64,
    /// Bytes por segundo.
    pub speed: f64,
    pub stage: Stage,
}

#[derive(Debug, Default)]
pub struct Session {
    pub phase: Phase,
    pub info: Info,
    pub progress: Option<Progress>,
    pub samples: Vec<Sample>,
    /// Formato e qualidade escolhidos, como "MP4 · 1080p".
    pub format: String,
    pub folder: String,
    /// Último arquivo concluído (caminho completo).
    pub last_file: Option<String>,
    started: Option<Instant>,
    /// Duração total, congelada quando o download termina.
    finished_after: Option<f64>,
}

impl Session {
    /// Começa um novo download, preservando o último arquivo concluído.
    pub fn begin(&mut self, format: String, folder: String) {
        *self = Session {
            phase: Phase::Starting,
            format,
            folder,
            last_file: self.last_file.take(),
            started: Some(Instant::now()),
            ..Session::default()
        };
    }

    /// Um novo item começou (o único, ou o próximo de uma playlist).
    pub fn set_info(&mut self, info: Info) {
        self.info = info;
        self.progress = None;
        self.phase = Phase::Downloading;
    }

    pub fn record(&mut self, progress: Progress) {
        let time = self.elapsed().unwrap_or(0.0);
        self.record_at(progress, time);
    }

    fn record_at(&mut self, progress: Progress, time: f64) {
        self.phase = Phase::Downloading;
        if let Some(speed) = progress.speed {
            let due = self
                .samples
                .last()
                .is_none_or(|last| time - last.time >= SAMPLE_INTERVAL);
            if due {
                self.samples.push(Sample {
                    time,
                    speed,
                    stage: progress.stage,
                });
                if self.samples.len() > MAX_SAMPLES {
                    // Mantém o período inteiro, com metade da resolução. A
                    // primeira e a última amostras são sempre preservadas.
                    let mut index = 0;
                    self.samples.retain(|_| {
                        index += 1;
                        index % 2 == 1
                    });
                }
            }
        }
        self.progress = Some(progress);
    }

    pub fn finish(&mut self, phase: Phase) {
        self.finished_after = self.elapsed();
        self.phase = phase;
    }

    /// Segundos desde o início do download.
    pub fn elapsed(&self) -> Option<f64> {
        self.finished_after
            .or_else(|| self.started.map(|started| started.elapsed().as_secs_f64()))
    }

    /// Porcentagem inteira do item atual.
    pub fn percent(&self) -> Option<u8> {
        self.progress
            .as_ref()
            .map(|progress| (progress.fraction * 100.0).floor().clamp(0.0, 100.0) as u8)
    }

    /// Velocidade média das amostras, em bytes por segundo.
    pub fn average_speed(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f64 = self.samples.iter().map(|sample| sample.speed).sum();
        Some(sum / self.samples.len() as f64)
    }

    pub fn peak_speed(&self) -> Option<f64> {
        self.samples
            .iter()
            .map(|sample| sample.speed)
            .fold(None, |peak: Option<f64>, speed| {
                Some(peak.map_or(speed, |peak| peak.max(speed)))
            })
    }
}

/// Tamanho legível, como "48,2 MB", no formato do idioma do sistema.
pub fn format_bytes(bytes: u64) -> String {
    glib::format_size(bytes).to_string()
}

/// Velocidade legível, como "2,3 MB/s".
pub fn format_speed(bytes_per_second: f64) -> String {
    format!("{}/s", format_bytes(bytes_per_second.max(0.0) as u64))
}

/// Duração como "0:07", "12:05" ou "1:02:03".
pub fn format_duration(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Barra de progresso em texto, para lugares que só aceitam texto (o menu
/// da bandeja): "▰▰▰▰▰▰▱▱▱▱".
pub fn text_bar(fraction: f64, width: usize) -> String {
    let filled = ((fraction.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    format!("{}{}", "▰".repeat(filled), "▱".repeat(width - filled))
}

/// "48,2 MB de 77,0 MB", ou só o que já foi baixado se o total for
/// desconhecido.
pub fn size_text(progress: &Progress) -> String {
    let downloaded = format_bytes(progress.downloaded);
    match progress.total {
        Some(total) => fill(
            &gettext("{downloaded} of {total}"),
            &[("downloaded", &downloaded), ("total", &format_bytes(total))],
        ),
        None => downloaded,
    }
}

/// "2,3 MB/s · restam 0:12", com o que estiver disponível.
pub fn rate_text(progress: &Progress) -> String {
    let mut parts = Vec::new();
    if let Some(speed) = progress.speed {
        parts.push(format_speed(speed));
    }
    if let Some(eta) = progress.eta {
        parts.push(fill(
            &gettext("{time} left"),
            &[("time", &format_duration(eta))],
        ));
    }
    parts.join(" · ")
}

impl Session {
    /// O que está acontecendo agora, em uma frase curta.
    pub fn activity(&self) -> String {
        match self.phase {
            Phase::Idle => gettext("No download in progress"),
            Phase::Starting => gettext("Fetching information…"),
            Phase::Downloading => match self.progress.as_ref().map(|progress| progress.stage) {
                Some(Stage::Video) => gettext("Downloading video"),
                Some(Stage::Audio) => gettext("Downloading audio"),
                _ => gettext("Downloading"),
            },
            Phase::Processing => gettext("Processing with FFmpeg…"),
            Phase::Done => gettext("Done"),
            Phase::Failed => gettext("Failed"),
            Phase::Cancelled => gettext("Cancelled"),
        }
    }

    /// "Item 3 de 12", quando o download é de uma playlist.
    pub fn playlist_text(&self) -> Option<String> {
        self.info.playlist.map(|(index, count)| {
            fill(
                &gettext("Item {index} of {count}"),
                &[("index", &index.to_string()), ("count", &count.to_string())],
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(fraction: f64, speed: Option<f64>) -> Progress {
        Progress {
            fraction,
            downloaded: (fraction * 1000.0) as u64,
            total: Some(1000),
            speed,
            eta: Some(3),
            stage: Stage::Video,
        }
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(7), "0:07");
        assert_eq!(format_duration(725), "12:05");
        assert_eq!(format_duration(3723), "1:02:03");
    }

    #[test]
    fn draws_text_bars() {
        assert_eq!(text_bar(0.0, 10), "▱▱▱▱▱▱▱▱▱▱");
        assert_eq!(text_bar(0.62, 10), "▰▰▰▰▰▰▱▱▱▱");
        assert_eq!(text_bar(1.7, 4), "▰▰▰▰");
    }

    #[test]
    fn samples_are_throttled() {
        let mut session = Session::default();
        session.record_at(progress(0.1, Some(100.0)), 0.0);
        session.record_at(progress(0.2, Some(200.0)), 0.1);
        session.record_at(progress(0.3, Some(300.0)), 0.3);
        // Sem velocidade não há ponto, mas o progresso é atualizado.
        session.record_at(progress(0.4, None), 1.0);

        let speeds: Vec<f64> = session.samples.iter().map(|sample| sample.speed).collect();
        assert_eq!(speeds, [100.0, 300.0]);
        assert_eq!(session.percent(), Some(40));
        assert_eq!(session.average_speed(), Some(200.0));
        assert_eq!(session.peak_speed(), Some(300.0));
    }

    #[test]
    fn sample_history_is_bounded() {
        let mut session = Session::default();
        for index in 0..2000 {
            session.record_at(progress(0.5, Some(index as f64)), index as f64);
        }
        assert!(session.samples.len() <= MAX_SAMPLES);
        // O período inteiro continua representado.
        assert!(session.samples.first().unwrap().time < 100.0);
        assert_eq!(session.samples.last().unwrap().time, 1999.0);
    }

    #[test]
    fn begin_keeps_the_last_file() {
        let mut session = Session {
            last_file: Some("/tmp/a.mp4".into()),
            ..Session::default()
        };
        session.record_at(progress(0.5, Some(1.0)), 0.0);
        session.begin("MP4".into(), "/tmp".into());
        assert_eq!(session.phase, Phase::Starting);
        assert!(session.samples.is_empty() && session.progress.is_none());
        assert_eq!(session.last_file.as_deref(), Some("/tmp/a.mp4"));
    }

    #[test]
    fn describes_progress() {
        let mut known = progress(0.5, Some(2_000_000.0));
        known.eta = Some(72);
        assert!(size_text(&known).contains(" of "));
        assert!(rate_text(&known).ends_with("1:12 left"));
        assert!(rate_text(&known).contains("/s · "));

        let unknown = Progress {
            total: None,
            speed: None,
            eta: None,
            ..known
        };
        assert!(!size_text(&unknown).contains(" of "));
        assert_eq!(rate_text(&unknown), "");
    }

    #[test]
    fn describes_activity_and_playlist() {
        let mut session = Session::default();
        assert_eq!(session.activity(), "No download in progress");
        assert_eq!(session.playlist_text(), None);

        session.set_info(Info {
            playlist: Some((3, 12)),
            ..Info::default()
        });
        session.record_at(progress(0.1, Some(1.0)), 0.0);
        assert_eq!(session.activity(), "Downloading video");
        assert_eq!(session.playlist_text().as_deref(), Some("Item 3 of 12"));
    }

    #[test]
    fn empty_session_has_no_numbers() {
        let session = Session::default();
        assert_eq!(session.percent(), None);
        assert_eq!(session.average_speed(), None);
        assert_eq!(session.peak_speed(), None);
        assert!(!session.phase.is_active());
    }
}
