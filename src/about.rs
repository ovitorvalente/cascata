//! Diálogo "Sobre o Cascata".

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config;
use crate::deps;

/// Novidades da versão atual, no subconjunto de marcação aceito pelo
/// `AdwAboutDialog`. Mantenha em sincronia com o `<release>` do metainfo.
fn release_notes() -> String {
    let items = [
        gettext("Video in MP4 with a maximum resolution of your choice, from 360p to 4K"),
        gettext("Audio only, converted to MP3"),
        gettext("Whole playlist downloads"),
        gettext("Installs yt-dlp and FFmpeg on Fedora and Ubuntu"),
    ];
    let mut notes = format!("<p>{}</p><ul>", gettext("First release of Cascata."));
    for item in items {
        notes.push_str(&format!("<li>{item}</li>"));
    }
    notes.push_str("</ul>");
    notes
}

pub fn show(parent: &impl IsA<gtk::Widget>) {
    let about = adw::AboutDialog::builder()
        .application_name(config::APP_NAME)
        .application_icon(config::APP_ID)
        .version(config::VERSION)
        .developer_name(config::DEVELOPER)
        .comments(gettext(
            "Download videos and audio from the internet. A front end for \
             yt-dlp and FFmpeg made with GTK 4 and libadwaita.",
        ))
        .website(config::WEBSITE)
        .issue_url(config::ISSUE_URL)
        .developers([format!(
            "{} {}",
            config::DEVELOPER,
            "https://github.com/ovitorvalente"
        )])
        .copyright(format!("© 2026 {}", config::DEVELOPER))
        .license_type(gtk::License::MitX11)
        .release_notes_version(config::VERSION)
        .release_notes(release_notes())
        .debug_info(debug_info(None))
        .debug_info_filename("cascata-debug.txt")
        .build();

    about.add_link(
        &gettext("yt-dlp documentation"),
        "https://github.com/yt-dlp/yt-dlp#readme",
    );
    about.add_link(
        &gettext("FFmpeg documentation"),
        "https://ffmpeg.org/documentation.html",
    );

    about.add_acknowledgement_section(
        Some(&gettext("Tools used")),
        &[
            "yt-dlp https://github.com/yt-dlp/yt-dlp",
            "FFmpeg https://ffmpeg.org",
        ],
    );

    // O Cascata não inclui estas ferramentas; ele executa as que estão
    // instaladas no sistema.
    about.add_legal_section(
        "yt-dlp",
        Some(&gettext("© yt-dlp contributors")),
        gtk::License::Custom,
        Some(&gettext(
            "Released under the <a href=\"https://unlicense.org\">Unlicense</a> \
             (public domain).",
        )),
    );
    about.add_legal_section(
        "FFmpeg",
        Some(&gettext("© FFmpeg developers")),
        gtk::License::Custom,
        Some(&gettext(
            "Released under the LGPL 2.1 or later; some builds include \
             components under the GPL. See \
             <a href=\"https://ffmpeg.org/legal.html\">ffmpeg.org/legal.html</a>.",
        )),
    );

    about.present(Some(parent));

    // Consultar as versões executa o yt-dlp e o ffmpeg, o que pode levar um
    // instante: o diálogo abre na hora e o texto é completado em seguida.
    glib::spawn_future_local(async move {
        if let Ok(tools) = gio::spawn_blocking(ToolVersions::query).await {
            about.set_debug_info(&debug_info(Some(&tools)));
        }
    });
}

struct ToolVersions {
    ytdlp: Option<String>,
    ffmpeg: Option<String>,
}

impl ToolVersions {
    fn query() -> Self {
        Self {
            ytdlp: deps::tool_version("yt-dlp", "--version"),
            ffmpeg: deps::tool_version("ffmpeg", "-version").map(|line| ffmpeg_version(&line)),
        }
    }
}

/// Texto da página "Solução de problemas". Precisa ser chamada na thread
/// principal, pois consulta as versões do GTK e do libadwaita.
fn debug_info(tools: Option<&ToolVersions>) -> String {
    let gtk_version = format!(
        "{}.{}.{}",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version()
    );
    let adw_version = format!(
        "{}.{}.{}",
        adw::major_version(),
        adw::minor_version(),
        adw::micro_version()
    );
    let system = deps::os_pretty_name().unwrap_or_else(|| gettext("unknown"));
    format_debug_info(tools, &gtk_version, &adw_version, &system)
}

/// Sem `tools`, as versões das ferramentas externas aparecem como
/// "verificando…".
fn format_debug_info(
    tools: Option<&ToolVersions>,
    gtk_version: &str,
    adw_version: &str,
    system: &str,
) -> String {
    let tool = |version: Option<&Option<String>>| match version {
        None => gettext("checking…"),
        Some(None) => gettext("not installed"),
        Some(Some(version)) => version.clone(),
    };

    format!(
        "{} {}\n\
         yt-dlp: {}\n\
         FFmpeg: {}\n\
         GTK: {gtk_version}\n\
         libadwaita: {adw_version}\n\
         System: {system}\n",
        config::APP_NAME,
        config::VERSION,
        tool(tools.map(|t| &t.ytdlp)),
        tool(tools.map(|t| &t.ffmpeg)),
    )
}

/// Reduz "ffmpeg version 7.1.1 Copyright (c) …" a "7.1.1".
fn ffmpeg_version(first_line: &str) -> String {
    let line = first_line.trim();
    let line = line.strip_prefix("ffmpeg version ").unwrap_or(line);
    line.split(" Copyright")
        .next()
        .unwrap_or(line)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ffmpeg_version() {
        assert_eq!(
            ffmpeg_version("ffmpeg version 7.1.1 Copyright (c) 2000-2025 the FFmpeg developers"),
            "7.1.1"
        );
        assert_eq!(ffmpeg_version("algo inesperado"), "algo inesperado");
    }

    #[test]
    fn debug_info_reports_tool_state() {
        let pending = format_debug_info(None, "4.22.5", "1.9.4", "Fedora Linux 44");
        assert!(pending.contains("yt-dlp: checking…"));

        let tools = ToolVersions {
            ytdlp: Some("2026.10.21".into()),
            ffmpeg: None,
        };
        let text = format_debug_info(Some(&tools), "4.22.5", "1.9.4", "Fedora Linux 44");
        assert!(text.contains("GTK: 4.22.5\nlibadwaita: 1.9.4\nSystem: Fedora Linux 44\n"));
        assert!(text.contains("yt-dlp: 2026.10.21"));
        assert!(text.contains("FFmpeg: not installed"));
        assert!(text.starts_with(&format!("{} {}", config::APP_NAME, config::VERSION)));
    }
}
