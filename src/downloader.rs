//! Executa o yt-dlp (que usa o FFmpeg para juntar/converter) e traduz a
//! saída dele em eventos para a interface.

use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

const TAG_PROGRESS: &str = "CASCATA_DL|";
const TAG_TITLE: &str = "CASCATA_TITLE|";
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

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Title(String),
    Progress {
        fraction: f64,
        speed: String,
        eta: String,
    },
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
        "download:CASCATA_DL|%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s",
        "--print",
        "before_dl:CASCATA_TITLE|%(title)s",
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

fn parse_line(line: &str) -> Option<Event> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix(TAG_PROGRESS) {
        let mut parts = rest.split('|');
        let percent = parts.next()?.trim().trim_end_matches('%').trim();
        let fraction = (percent.parse::<f64>().ok()? / 100.0).clamp(0.0, 1.0);
        let speed = clean_field(parts.next().unwrap_or(""));
        let eta = clean_field(parts.next().unwrap_or(""));
        Some(Event::Progress {
            fraction,
            speed,
            eta,
        })
    } else if let Some(rest) = line.strip_prefix(TAG_TITLE) {
        Some(Event::Title(rest.to_string()))
    } else if line.starts_with(TAG_POST) {
        Some(Event::PostProcessing)
    } else {
        line.strip_prefix(TAG_FILE)
            .map(|rest| Event::File(rest.to_string()))
    }
}

/// O yt-dlp escreve "Unknown"/"NA" quando não sabe o valor.
fn clean_field(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value.contains("Unknown") || value == "NA" {
        String::new()
    } else {
        value.to_string()
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
        "O yt-dlp terminou com erro.".to_string()
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

        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer);
                    if let Some(event) = parse_line(&line) {
                        let _ = sender.send_blocking(event);
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

    #[test]
    fn parses_progress() {
        let event = parse_line("CASCATA_DL|  45.3%|   2.31MiB/s|00:12\n").unwrap();
        match event {
            Event::Progress {
                fraction,
                speed,
                eta,
            } => {
                assert!((fraction - 0.453).abs() < 1e-9);
                assert_eq!(speed, "2.31MiB/s");
                assert_eq!(eta, "00:12");
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn unknown_fields_become_empty() {
        let event = parse_line("CASCATA_DL|100.0%|Unknown B/s|Unknown").unwrap();
        assert_eq!(
            event,
            Event::Progress {
                fraction: 1.0,
                speed: String::new(),
                eta: String::new()
            }
        );
    }

    #[test]
    fn parses_other_events() {
        assert_eq!(
            parse_line("CASCATA_TITLE|Um vídeo | com barra"),
            Some(Event::Title("Um vídeo | com barra".into()))
        );
        assert_eq!(parse_line("CASCATA_PP|"), Some(Event::PostProcessing));
        assert_eq!(
            parse_line("CASCATA_FILE|/tmp/a.mp4"),
            Some(Event::File("/tmp/a.mp4".into()))
        );
        assert_eq!(parse_line("[download] qualquer coisa"), None);
        assert_eq!(parse_line("CASCATA_DL|N/A|x|y"), None);
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
        let (mut title, mut progress, mut file, mut outcome) = (false, 0, None, None);
        while let Ok(event) = events.recv_blocking() {
            match event {
                Event::Title(t) => {
                    println!("título: {t}");
                    title = true;
                }
                Event::Progress { .. } => progress += 1,
                Event::PostProcessing => println!("pós-processamento"),
                Event::File(f) => {
                    println!("arquivo: {f}");
                    file = Some(f);
                }
                Event::Finished(o) => outcome = Some(o),
            }
        }
        println!("eventos de progresso: {progress}");
        assert_eq!(outcome, Some(Outcome::Done));
        assert!(title, "o título deveria ter sido informado");
        assert!(progress > 0, "deveria haver eventos de progresso");
        let file = PathBuf::from(file.expect("o arquivo final deveria ter sido informado"));
        assert!(file.starts_with(&folder) && file.exists());
        let expected = if kind == Kind::Audio { "mp3" } else { "mp4" };
        assert_eq!(file.extension().and_then(|e| e.to_str()), Some(expected));
    }

    #[test]
    fn picks_last_error_line() {
        let stderr = "WARNING: algo\nERROR: primeiro\nERROR: [youtube] abc: Video unavailable\n";
        assert_eq!(error_message(stderr), "[youtube] abc: Video unavailable");
        assert_eq!(error_message(""), "O yt-dlp terminou com erro.");
    }
}
