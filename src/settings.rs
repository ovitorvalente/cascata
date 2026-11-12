//! Configurações do aplicativo, guardadas em
//! `~/.config/cascata/settings.ini`.

use std::fs;
use std::path::PathBuf;

use gtk::glib;

use crate::downloader::Kind;

const GROUP_GENERAL: &str = "general";
const GROUP_DOWNLOADS: &str = "downloads";
const GROUP_BACKGROUND: &str = "background";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// Segue o idioma do sistema.
    #[default]
    System,
    English,
    Portuguese,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::System, Language::English, Language::Portuguese];

    /// Valor para a variável `LANGUAGE` do gettext; `None` = não alterar.
    pub fn code(self) -> Option<&'static str> {
        match self {
            Language::System => None,
            Language::English => Some("en"),
            Language::Portuguese => Some("pt_BR"),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Language::System => "system",
            Language::English => "en",
            Language::Portuguese => "pt_BR",
        }
    }

    fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|language| language.key() == key)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub language: Language,
    /// Formato pré-selecionado para novos downloads.
    pub kind: Kind,
    /// Altura máxima pré-selecionada; `None` = melhor disponível.
    pub max_height: Option<u32>,
    /// Pasta de destino; `None` = pasta de Downloads do usuário.
    pub folder: Option<PathBuf>,
    /// Continuar o download ao fechar a janela.
    pub background: bool,
    /// Mostrar o ícone na bandeja do sistema.
    pub tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::System,
            kind: Kind::Video,
            max_height: None,
            folder: None,
            background: true,
            tray: false,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        glib::user_config_dir().join("cascata").join("settings.ini")
    }

    /// Lê as configurações salvas; o que faltar ou for inválido usa o padrão.
    pub fn load() -> Self {
        let file = glib::KeyFile::new();
        match file.load_from_file(Self::path(), glib::KeyFileFlags::NONE) {
            Ok(()) => Self::from_key_file(&file),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
        fs::write(&path, self.to_key_file().to_data().as_str()).map_err(|err| err.to_string())
    }

    /// Pasta de destino efetiva.
    pub fn download_folder(&self) -> PathBuf {
        self.folder.clone().unwrap_or_else(|| {
            glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(glib::home_dir)
        })
    }

    fn from_key_file(file: &glib::KeyFile) -> Self {
        let defaults = Self::default();

        let language = file
            .string(GROUP_GENERAL, "language")
            .map(|key| Language::from_key(&key))
            .unwrap_or(defaults.language);
        let kind = match file.string(GROUP_DOWNLOADS, "format").as_deref() {
            Ok("audio") => Kind::Audio,
            _ => Kind::Video,
        };
        let max_height = file
            .integer(GROUP_DOWNLOADS, "max-height")
            .ok()
            .and_then(|height| u32::try_from(height).ok())
            .filter(|height| *height > 0);
        let folder = file
            .string(GROUP_DOWNLOADS, "folder")
            .ok()
            .filter(|folder| !folder.is_empty())
            .map(|folder| PathBuf::from(folder.as_str()));
        let background = file
            .boolean(GROUP_BACKGROUND, "keep-downloading")
            .unwrap_or(defaults.background);
        let tray = file
            .boolean(GROUP_BACKGROUND, "tray-icon")
            .unwrap_or(defaults.tray);

        Self {
            language,
            kind,
            max_height,
            folder,
            background,
            tray,
        }
    }

    fn to_key_file(&self) -> glib::KeyFile {
        let file = glib::KeyFile::new();
        file.set_string(GROUP_GENERAL, "language", self.language.key());
        file.set_string(
            GROUP_DOWNLOADS,
            "format",
            match self.kind {
                Kind::Video => "video",
                Kind::Audio => "audio",
            },
        );
        file.set_integer(
            GROUP_DOWNLOADS,
            "max-height",
            self.max_height
                .and_then(|height| i32::try_from(height).ok())
                .unwrap_or(0),
        );
        file.set_string(
            GROUP_DOWNLOADS,
            "folder",
            &self
                .folder
                .as_ref()
                .map(|folder| folder.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        file.set_boolean(GROUP_BACKGROUND, "keep-downloading", self.background);
        file.set_boolean(GROUP_BACKGROUND, "tray-icon", self.tray);
        file
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_a_round_trip() {
        let settings = Settings {
            language: Language::Portuguese,
            kind: Kind::Audio,
            max_height: Some(1080),
            folder: Some(PathBuf::from("/tmp/vídeos baixados")),
            background: false,
            tray: true,
        };
        assert_eq!(Settings::from_key_file(&settings.to_key_file()), settings);

        let defaults = Settings::default();
        assert_eq!(Settings::from_key_file(&defaults.to_key_file()), defaults);
    }

    #[test]
    fn missing_or_invalid_values_use_defaults() {
        let file = glib::KeyFile::new();
        assert_eq!(Settings::from_key_file(&file), Settings::default());

        file.load_from_data(
            "[general]\nlanguage=klingon\n[downloads]\nformat=??\nmax-height=-5\nfolder=\n",
            glib::KeyFileFlags::NONE,
        )
        .unwrap();
        assert_eq!(Settings::from_key_file(&file), Settings::default());
    }

    #[test]
    fn language_codes() {
        assert_eq!(Language::System.code(), None);
        assert_eq!(Language::English.code(), Some("en"));
        assert_eq!(Language::Portuguese.code(), Some("pt_BR"));
        assert_eq!(Language::from_key("pt_BR"), Language::Portuguese);
    }
}
