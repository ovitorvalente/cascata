//! Detecção e instalação das ferramentas externas (yt-dlp e FFmpeg).

use std::fs;
use std::process::{Command, Stdio};

use gtk::{gio, glib};

/// Quais ferramentas estão disponíveis no PATH.
#[derive(Debug, Clone, Copy)]
pub struct Status {
    pub ytdlp: bool,
    pub ffmpeg: bool,
}

impl Status {
    pub fn all_present(self) -> bool {
        self.ytdlp && self.ffmpeg
    }

    /// Nomes legíveis do que está faltando.
    pub fn missing_names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if !self.ytdlp {
            names.push("yt-dlp");
        }
        if !self.ffmpeg {
            names.push("FFmpeg");
        }
        names
    }
}

pub fn check() -> Status {
    Status {
        ytdlp: glib::find_program_in_path("yt-dlp").is_some(),
        ffmpeg: glib::find_program_in_path("ffmpeg").is_some(),
    }
}

/// Primeira linha da saída de `<programa> <argumento>`, usada para descobrir
/// a versão de uma ferramenta. `None` se ela não puder ser executada.
pub fn tool_version(program: &str, arg: &str) -> Option<String> {
    let output = Command::new(program)
        .arg(arg)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Nome legível do sistema, como "Fedora Linux 44 (Workstation Edition)".
pub fn os_pretty_name() -> Option<String> {
    let content = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .ok()?;
    parse_pretty_name(&content)
}

fn parse_pretty_name(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    Fedora,
    Ubuntu,
    Unknown,
}

pub fn detect_distro() -> Distro {
    let content = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();
    parse_os_release(&content)
}

fn parse_os_release(content: &str) -> Distro {
    let mut ids: Vec<String> = Vec::new();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "ID" || key == "ID_LIKE" {
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            ids.extend(value.split_whitespace().map(str::to_lowercase));
        }
    }

    if ids.iter().any(|id| id == "fedora") {
        Distro::Fedora
    } else if ids.iter().any(|id| id == "ubuntu" || id == "debian") {
        Distro::Ubuntu
    } else {
        Distro::Unknown
    }
}

/// Comando de instalação pronto para ser executado via `pkexec`.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// Argumentos passados ao `pkexec`.
    argv: Vec<String>,
    /// Versão legível, mostrada ao usuário antes de pedir a senha.
    pub display: String,
}

/// Monta o comando de instalação para o que falta, conforme a distribuição.
/// Retorna `None` se nada falta ou se a distribuição não é suportada.
pub fn install_plan(status: Status, distro: Distro) -> Option<InstallPlan> {
    if status.all_present() {
        return None;
    }

    match distro {
        Distro::Fedora => {
            let mut packages = Vec::new();
            if !status.ytdlp {
                packages.push("yt-dlp");
            }
            if !status.ffmpeg {
                // Disponível nos repositórios oficiais do Fedora (sem RPM Fusion).
                packages.push("ffmpeg-free");
            }
            let mut argv: Vec<String> = vec!["dnf".into(), "install".into(), "-y".into()];
            argv.extend(packages.iter().map(|p| p.to_string()));
            let display = format!("dnf install -y {}", packages.join(" "));
            Some(InstallPlan { argv, display })
        }
        Distro::Ubuntu => {
            let mut packages = Vec::new();
            if !status.ytdlp {
                packages.push("yt-dlp");
            }
            if !status.ffmpeg {
                packages.push("ffmpeg");
            }
            let display = format!(
                "apt-get update && apt-get install -y {}",
                packages.join(" ")
            );
            let script = format!(
                "export DEBIAN_FRONTEND=noninteractive; apt-get update && apt-get install -y {}",
                packages.join(" ")
            );
            let argv = vec!["sh".into(), "-c".into(), script];
            Some(InstallPlan { argv, display })
        }
        Distro::Unknown => None,
    }
}

/// Executa a instalação com privilégios de administrador (polkit / pkexec),
/// sem travar a interface.
pub async fn install(plan: InstallPlan) -> Result<(), String> {
    let task = gio::spawn_blocking(move || {
        let output = Command::new("pkexec")
            .args(&plan.argv)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("Não foi possível executar o pkexec: {err}"))?;

        if output.status.success() {
            return Ok(());
        }

        match output.status.code() {
            // Códigos documentados em pkexec(1).
            Some(126) => Err("A autenticação foi cancelada.".to_string()),
            Some(127) => Err("Autorização negada para instalar pacotes.".to_string()),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let text = if stderr.trim().is_empty() {
                    stdout
                } else {
                    stderr
                };
                Err(last_lines(&text, 8))
            }
        }
    });

    task.await
        .unwrap_or_else(|_| Err("A instalação foi interrompida inesperadamente.".to_string()))
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(count);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        "O gerenciador de pacotes terminou com erro.".to_string()
    } else {
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_fedora() {
        let os = "NAME=\"Fedora Linux\"\nID=fedora\nVERSION_ID=44\n";
        assert_eq!(parse_os_release(os), Distro::Fedora);
    }

    #[test]
    fn detects_ubuntu_and_derivatives() {
        assert_eq!(
            parse_os_release("ID=ubuntu\nID_LIKE=debian\n"),
            Distro::Ubuntu
        );
        assert_eq!(
            parse_os_release("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"),
            Distro::Ubuntu
        );
    }

    #[test]
    fn reads_pretty_name() {
        let os = "NAME=\"Fedora Linux\"\nPRETTY_NAME=\"Fedora Linux 44 (Workstation Edition)\"\n";
        assert_eq!(
            parse_pretty_name(os).as_deref(),
            Some("Fedora Linux 44 (Workstation Edition)")
        );
        assert_eq!(parse_pretty_name("ID=fedora\n"), None);
    }

    #[test]
    fn unknown_distro() {
        assert_eq!(parse_os_release("ID=arch\n"), Distro::Unknown);
        assert_eq!(parse_os_release(""), Distro::Unknown);
    }

    #[test]
    fn plan_only_installs_what_is_missing() {
        let status = Status {
            ytdlp: false,
            ffmpeg: true,
        };
        let plan = install_plan(status, Distro::Fedora).unwrap();
        assert_eq!(plan.argv, ["dnf", "install", "-y", "yt-dlp"]);

        let plan = install_plan(status, Distro::Ubuntu).unwrap();
        assert!(plan.argv[2].ends_with("apt-get install -y yt-dlp"));

        let all = Status {
            ytdlp: true,
            ffmpeg: true,
        };
        assert!(install_plan(all, Distro::Fedora).is_none());
        assert!(install_plan(status, Distro::Unknown).is_none());
    }
}
