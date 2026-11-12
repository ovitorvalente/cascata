//! Executa o yt-dlp (que usa o FFmpeg para juntar/converter) e traduz a
//! saída dele em eventos para a interface.

use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use gettextrs::gettext;

const TAG_PROGRESS: &str = "CASCATA_DL|";
const TAG_INFO: &str = "CASCATA_INFO|";
const TAG_POST: &str = "CASCATA_PP|";
const TAG_FILE: &str = "CASCATA_FILE|";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub url: String,
    pub kind: Kind,
    /// Altura máxima do vídeo (ex.: 1080). `None` = melhor disponível.
    pub max_height: Option<u32>,
    pub playlist: bool,
    pub folder: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done,
    Cancelled,
    Failed(String),
}

/// O que está sendo baixado no momento. Vídeo e áudio costumam vir em
/// fluxos separados, baixados um depois do outro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Video,
    Audio,
    /// Um único arquivo com vídeo e áudio.
    Combined,
}

/// Dados do item, conhecidos antes de o download começar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info {
    pub title: String,
    pub thumbnail: Option<String>,
    /// Posição e total, quando o item faz parte de uma playlist.
    pub playlist: Option<(u32, u32)>,
    /// Duração em segundos.
    pub duration: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// Progresso do item inteiro (todos os fluxos), de 0 a 1.
    pub fraction: f64,
    /// Bytes já baixados do item inteiro.
    pub downloaded: u64,
    /// Tamanho total estimado do item, se conhecido.
    pub total: Option<u64>,
    /// Bytes por segundo.
    pub speed: Option<f64>,
    /// Segundos restantes do fluxo atual.
    pub eta: Option<u64>,
    pub stage: Stage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Info(Info),
    Progress(Progress),
    PostProcessing,
    File(String),
    Finished(Outcome),
}

/// Controle de um download em andamento.
pub struct Handle {
    pid: u32,
    cancelled: Arc<AtomicBool>,
}

impl Handle {
    /// Encerra o yt-dlp e os filhos dele (ffmpeg), que estão no mesmo grupo
    /// de processos.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let _ = Command::new("kill")
            .args(["-s", "TERM", "--", &format!("-{}", self.pid)])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn build_args(req: &Request) -> Vec<String> {
    let mut args: Vec<String> = [
        "--newline",
        "--progress",
        "--color",
        "never",
        "--no-warnings",
        "--progress-template",
        "download:CASCATA_DL|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.vcodec)s|%(info.acodec)s",
        "--print",
        // O título vem por último porque pode conter "|".
        "before_dl:CASCATA_INFO|%(playlist_index)s|%(n_entries)s|%(duration)s|%(filesize_approx)s|%(thumbnail)s|%(title)s",
        "--print",
        "post_process:CASCATA_PP|",
        "--print",
        "after_move:CASCATA_FILE|%(filepath)s",
        "-o",
        "%(title)s.%(ext)s",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    args.push("-P".into());
    args.push(req.folder.to_string_lossy().into_owned());

    args.push(if req.playlist {
        "--yes-playlist".into()
    } else {
        "--no-playlist".into()
    });

    match req.kind {
        Kind::Video => {
            let format = match req.max_height {
                Some(h) => format!("bv*[height<={h}]+ba/b[height<={h}]/b"),
                None => "bv*+ba/b".to_string(),
            };
            args.push("-f".into());
            args.push(format);
            args.push("--merge-output-format".into());
            args.push("mp4".into());
        }
        Kind::Audio => {
            args.extend(
                [
                    "-f",
                    "ba/b",
                    "-x",
                    "--audio-format",
                    "mp3",
                    "--audio-quality",
                    "0",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
        }
    }

    // "--" impede que um endereço começando com "-" seja lido como opção.
    args.push("--".into());
    args.push(req.url.clone());
    args
}

/// Uma linha da saída do yt-dlp, ainda sem o contexto das anteriores.
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Info {
        info: Info,
        /// Tamanho aproximado do item inteiro.
        size: Option<u64>,
    },
    Progress {
        /// Bytes baixados do fluxo atual.
        downloaded: u64,
        /// Tamanho do fluxo atual.
        total: Option<u64>,
        speed: Option<f64>,
        eta: Option<u64>,
        stage: Stage,
    },
    PostProcessing,
    File(String),
}

/// O yt-dlp escreve "NA" (ou "None") quando não sabe um valor.
fn number(field: &str) -> Option<f64> {
    field
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn text(field: &str) -> Option<String> {
    let field = field.trim();
    (!field.is_empty() && field != "NA" && field != "None").then(|| field.to_string())
}

fn parse_line(line: &str) -> Option<Line> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix(TAG_PROGRESS) {
        let fields: Vec<&str> = rest.split('|').collect();
        let field = |index: usize| fields.get(index).copied().unwrap_or("");

        let downloaded = number(field(0))? as u64;
        let total = number(field(1)).or_else(|| number(field(2)));
        let has_video = text(field(5)).is_some_and(|codec| codec != "none");
        let has_audio = text(field(6)).is_some_and(|codec| codec != "none");
        let stage = match (has_video, has_audio) {
            (true, false) => Stage::Video,
            (false, true) => Stage::Audio,
            _ => Stage::Combined,
        };
        Some(Line::Progress {
            downloaded,
            total: total.map(|total| total as u64),
            speed: number(field(3)).filter(|speed| *speed > 0.0),
            eta: number(field(4)).map(|eta| eta as u64),
            stage,
        })
    } else if let Some(rest) = line.strip_prefix(TAG_INFO) {
        let mut fields = rest.splitn(6, '|');
        let index = fields.next().and_then(number);
        let count = fields.next().and_then(number);
        let duration = fields.next().and_then(number);
        let size = fields.next().and_then(number);
        let thumbnail = fields.next().and_then(text);
        let title = fields.next().unwrap_or("").to_string();
        let playlist = match (index, count) {
            (Some(index), Some(count)) if count >= 1.0 => Some((index as u32, count as u32)),
            _ => None,
        };
        Some(Line::Info {
            info: Info {
                title,
                thumbnail,
                playlist,
                duration: duration.map(|duration| duration as u64),
            },
            size: size.map(|size| size as u64).filter(|size| *size > 0),
        })
    } else if line.starts_with(TAG_POST) {
        Some(Line::PostProcessing)
    } else {
        line.strip_prefix(TAG_FILE)
            .map(|rest| Line::File(rest.to_string()))
    }
}

/// Transforma o progresso de cada fluxo (vídeo, depois áudio) no progresso
/// do item inteiro, para a porcentagem não voltar a zero entre os fluxos.
#[derive(Debug, Default)]
struct Tracker {
    /// Tamanho aproximado do item inteiro, informado antes do download.
    size: Option<u64>,
    /// Bytes dos fluxos já concluídos.
    finished: u64,
    stream_downloaded: u64,
    stream_total: u64,
    stage: Option<Stage>,
    fraction: f64,
}

impl Tracker {
    fn event(&mut self, line: Line) -> Event {
        match line {
            Line::Info { info, size } => {
                *self = Tracker {
                    size,
                    ..Tracker::default()
                };
                Event::Info(info)
            }
            Line::Progress {
                downloaded,
                total,
                speed,
                eta,
                stage,
            } => {
                let new_stream = self.stage.is_some_and(|previous| previous != stage)
                    || downloaded < self.stream_downloaded;
                if new_stream {
                    self.finished += self.stream_total.max(self.stream_downloaded);
                }
                self.stage = Some(stage);
                self.stream_downloaded = downloaded;
                self.stream_total = total.unwrap_or(downloaded).max(downloaded);

                let done = self.finished + downloaded;
                let known = self.finished + self.stream_total;
                // O tamanho informado antes é só uma estimativa; nunca pode
                // ser menor do que o que já se sabe dos fluxos.
                let overall = match self.size {
                    Some(size) => Some(size.max(known)),
                    None => total.map(|_| known),
                };
                let fraction = match overall {
                    Some(overall) if overall > 0 => (done as f64 / overall as f64).min(1.0),
                    _ => 0.0,
                };
                // Sem estimativa do item inteiro, o total cresce quando o
                // segundo fluxo começa; a porcentagem só não pode recuar.
                self.fraction = self.fraction.max(fraction);

                Event::Progress(Progress {
                    fraction: self.fraction,
                    downloaded: done,
                    total: overall,
                    speed,
                    eta,
                    stage,
                })
            }
            Line::PostProcessing => Event::PostProcessing,
            Line::File(path) => Event::File(path),
        }
    }
}

/// Extrai a mensagem mais útil do stderr do yt-dlp.
fn error_message(stderr: &str) -> String {
    let errors: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("ERROR:"))
        .collect();
    if let Some(last) = errors.last() {
        return last.trim_start_matches("ERROR:").trim().to_string();
    }
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(5);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        gettext("yt-dlp exited with an error.")
    } else {
        tail
    }
}

/// Inicia o download. Os eventos chegam pelo canal retornado; o último é
/// sempre `Event::Finished`.
pub fn start(req: Request) -> io::Result<(Handle, async_channel::Receiver<Event>)> {
    let mut child = Command::new("yt-dlp")
        .args(build_args(&req))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Grupo próprio, para o cancelamento alcançar também o ffmpeg.
        .process_group(0)
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .expect("stdout foi configurado como pipe");
    let mut stderr = child
        .stderr
        .take()
        .expect("stderr foi configurado como pipe");

    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = Handle {
        pid: child.id(),
        cancelled: cancelled.clone(),
    };

    let (sender, receiver) = async_channel::unbounded();

    thread::spawn(move || {
        // Lê o stderr em paralelo para o processo nunca travar com o pipe cheio.
        let stderr_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        });

        let mut tracker = Tracker::default();
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer);
                    if let Some(line) = parse_line(&line) {
                        let _ = sender.send_blocking(tracker.event(line));
                    }
                }
            }
        }

        let status = child.wait();
        let stderr_text = stderr_reader.join().unwrap_or_default();

        let outcome = if cancelled.load(Ordering::SeqCst) {
            Outcome::Cancelled
        } else {
            match status {
                Ok(status) if status.success() => Outcome::Done,
                Ok(_) => Outcome::Failed(error_message(&stderr_text)),
                Err(err) => Outcome::Failed(err.to_string()),
            }
        };
        let _ = sender.send_blocking(Event::Finished(outcome));
    });

    Ok((handle, receiver))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: Kind, max_height: Option<u32>) -> Request {
        Request {
            url: "https://example.com/v".into(),
            kind,
            max_height,
            playlist: false,
            folder: PathBuf::from("/tmp/saida"),
        }
    }

    fn progress(event: Event) -> Progress {
        match event {
            Event::Progress(progress) => progress,
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn parses_progress() {
        // Linha real do yt-dlp para um fluxo só de vídeo.
        let line = "CASCATA_DL|130048|223779|NA|6155748.18|0|av01.0.00M.08|none\n";
        assert_eq!(
            parse_line(line),
            Some(Line::Progress {
                downloaded: 130048,
                total: Some(223779),
                speed: Some(6155748.18),
                eta: Some(0),
                stage: Stage::Video,
            })
        );
    }

    #[test]
    fn unknown_fields_become_none() {
        let line = parse_line("CASCATA_DL|1024|NA|2048.5|NA|NA|none|opus").unwrap();
        assert_eq!(
            line,
            Line::Progress {
                downloaded: 1024,
                // Sem o total exato, vale a estimativa.
                total: Some(2048),
                speed: None,
                eta: None,
                stage: Stage::Audio,
            }
        );
        assert_eq!(parse_line("CASCATA_DL|NA|NA|NA|NA|NA|NA|NA"), None);
    }

    #[test]
    fn parses_info() {
        let line = "CASCATA_INFO|3|12|19|475961|https://example.com/t.jpg|Um vídeo | com barra";
        assert_eq!(
            parse_line(line),
            Some(Line::Info {
                info: Info {
                    title: "Um vídeo | com barra".into(),
                    thumbnail: Some("https://example.com/t.jpg".into()),
                    playlist: Some((3, 12)),
                    duration: Some(19),
                },
                size: Some(475961),
            })
        );

        let single = parse_line("CASCATA_INFO|NA|NA|NA|NA|NA|Sem dados").unwrap();
        assert_eq!(
            single,
            Line::Info {
                info: Info {
                    title: "Sem dados".into(),
                    ..Info::default()
                },
                size: None,
            }
        );
    }

    #[test]
    fn parses_other_lines() {
        assert_eq!(parse_line("CASCATA_PP|"), Some(Line::PostProcessing));
        assert_eq!(
            parse_line("CASCATA_FILE|/tmp/a.mp4"),
            Some(Line::File("/tmp/a.mp4".into()))
        );
        assert_eq!(parse_line("[download] qualquer coisa"), None);
    }

    fn stream(downloaded: u64, total: u64, stage: Stage) -> Line {
        Line::Progress {
            downloaded,
            total: Some(total),
            speed: Some(1000.0),
            eta: Some(1),
            stage,
        }
    }

    #[test]
    fn tracks_overall_progress_across_streams() {
        let mut tracker = Tracker::default();
        tracker.event(Line::Info {
            info: Info::default(),
            size: Some(1000),
        });

        let video = progress(tracker.event(stream(300, 600, Stage::Video)));
        assert_eq!(
            (video.downloaded, video.total, video.fraction),
            (300, Some(1000), 0.3)
        );

        progress(tracker.event(stream(600, 600, Stage::Video)));
        // O áudio começa do zero, mas o item continua de onde estava.
        let audio = progress(tracker.event(stream(200, 400, Stage::Audio)));
        assert_eq!(
            (audio.downloaded, audio.total, audio.fraction),
            (800, Some(1000), 0.8)
        );
        let end = progress(tracker.event(stream(400, 400, Stage::Audio)));
        assert_eq!(end.fraction, 1.0);
    }

    #[test]
    fn progress_never_goes_backwards_without_an_estimate() {
        let mut tracker = Tracker::default();
        let video = progress(tracker.event(stream(600, 600, Stage::Video)));
        assert_eq!(video.fraction, 1.0);
        let audio = progress(tracker.event(stream(100, 400, Stage::Audio)));
        assert_eq!(audio.fraction, 1.0);
        assert_eq!((audio.downloaded, audio.total), (700, Some(1000)));
    }

    #[test]
    fn a_new_item_resets_the_tracker() {
        let mut tracker = Tracker::default();
        progress(tracker.event(stream(600, 600, Stage::Combined)));
        tracker.event(Line::Info {
            info: Info::default(),
            size: None,
        });
        let next = progress(tracker.event(stream(100, 400, Stage::Combined)));
        assert_eq!((next.downloaded, next.fraction), (100, 0.25));
    }

    #[test]
    fn video_args() {
        let args = build_args(&request(Kind::Video, Some(1080)));
        assert!(args.contains(&"bv*[height<=1080]+ba/b[height<=1080]/b".to_string()));
        assert!(args.contains(&"--merge-output-format".to_string()));
        assert!(args.contains(&"--no-playlist".to_string()));
        assert_eq!(args[args.len() - 2], "--");
        assert_eq!(args[args.len() - 1], "https://example.com/v");
    }

    #[test]
    fn audio_args() {
        let args = build_args(&request(Kind::Audio, None));
        assert!(args.contains(&"-x".to_string()));
        assert!(args.contains(&"mp3".to_string()));
        assert!(!args.contains(&"--merge-output-format".to_string()));
    }

    /// Teste de ponta a ponta (usa a rede, o yt-dlp e o FFmpeg reais):
    /// CASCATA_TEST_URL=<endereço> cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_download() {
        let url = std::env::var("CASCATA_TEST_URL").expect("defina CASCATA_TEST_URL");
        let kind = match std::env::var("CASCATA_TEST_KIND").as_deref() {
            Ok("audio") => Kind::Audio,
            _ => Kind::Video,
        };
        let folder = std::env::temp_dir().join("cascata-teste");
        let mut req = request(kind, Some(360));
        req.url = url;
        req.folder = folder.clone();

        let (_handle, events) = start(req).expect("yt-dlp precisa estar instalado");
        let (mut info, mut updates, mut file, mut outcome) = (None, Vec::new(), None, None);
        while let Ok(event) = events.recv_blocking() {
            match event {
                Event::Info(i) => {
                    println!("informações: {i:?}");
                    info = Some(i);
                }
                Event::Progress(p) => updates.push(p),
                Event::PostProcessing => println!("pós-processamento"),
                Event::File(f) => {
                    println!("arquivo: {f}");
                    file = Some(f);
                }
                Event::Finished(o) => outcome = Some(o),
            }
        }
        println!("eventos de progresso: {}", updates.len());
        if let Some(last) = updates.last() {
            println!("último: {last:?}");
        }
        assert_eq!(outcome, Some(Outcome::Done));
        assert!(!info
            .expect("as informações deveriam ter chegado")
            .title
            .is_empty());
        assert!(!updates.is_empty(), "deveria haver eventos de progresso");
        assert!(
            updates
                .windows(2)
                .all(|pair| pair[0].fraction <= pair[1].fraction),
            "a porcentagem não pode recuar"
        );
        assert!(updates.last().unwrap().fraction > 0.9);
        let file = PathBuf::from(file.expect("o arquivo final deveria ter sido informado"));
        assert!(file.starts_with(&folder) && file.exists());
        let expected = if kind == Kind::Audio { "mp3" } else { "mp4" };
        assert_eq!(file.extension().and_then(|e| e.to_str()), Some(expected));
    }

    #[test]
    fn picks_last_error_line() {
        let stderr = "WARNING: algo\nERROR: primeiro\nERROR: [youtube] abc: Video unavailable\n";
        assert_eq!(error_message(stderr), "[youtube] abc: Video unavailable");
        assert_eq!(error_message(""), "yt-dlp exited with an error.");
    }
}
