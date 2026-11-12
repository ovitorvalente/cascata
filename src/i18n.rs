//! Tradução da interface com gettext.
//!
//! As mensagens no código estão em inglês (o idioma de origem); as
//! traduções ficam em `po/<idioma>.po` e são instaladas como
//! `<localedir>/<idioma>/LC_MESSAGES/cascata.mo`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain};
use gtk::glib;

pub const DOMAIN: &str = "cascata";

/// Associa o catálogo de traduções ao aplicativo. Deve ser chamada antes de
/// criar qualquer widget. O idioma em si é definido pelo GTK, que chama
/// `setlocale` com as configurações do sistema ao iniciar.
pub fn init() {
    if let Some(dir) = locale_dir() {
        let _ = bindtextdomain(DOMAIN, dir);
    }
    let _ = bind_textdomain_codeset(DOMAIN, "UTF-8");
    let _ = textdomain(DOMAIN);
}

/// Procura a pasta que contém os catálogos do Cascata, nesta ordem:
/// a variável de ambiente `CASCATA_LOCALEDIR`, a pasta definida na
/// compilação pelo Meson, a pasta de dados do usuário e as do sistema.
fn locale_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = env::var_os("CASCATA_LOCALEDIR") {
        candidates.push(dir.into());
    }
    if let Some(dir) = option_env!("CASCATA_LOCALEDIR") {
        candidates.push(dir.into());
    }
    candidates.push(glib::user_data_dir().join("locale"));
    candidates.extend(
        glib::system_data_dirs()
            .into_iter()
            .map(|d| d.join("locale")),
    );

    candidates.into_iter().find(|dir| has_catalog(dir))
}

fn has_catalog(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .join("LC_MESSAGES")
            .join(format!("{DOMAIN}.mo"))
            .is_file()
    })
}

/// Substitui marcadores `{nome}` em uma mensagem já traduzida. É usada no
/// lugar de `format!`, que só aceita textos fixos em tempo de compilação.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut text = template.to_string();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_named_placeholders() {
        assert_eq!(
            fill("{a} and {b}, then {a}", &[("a", "x"), ("b", "y")]),
            "x and y, then x"
        );
        assert_eq!(fill("no placeholders", &[("a", "x")]), "no placeholders");
    }

    #[test]
    fn finds_catalog_only_where_it_exists() {
        let base = env::temp_dir().join(format!("cascata-i18n-{}", std::process::id()));
        let messages = base.join("pt_BR").join("LC_MESSAGES");
        fs::create_dir_all(&messages).unwrap();
        assert!(!has_catalog(&base));
        fs::write(messages.join("cascata.mo"), b"").unwrap();
        assert!(has_catalog(&base));
        assert!(!has_catalog(&base.join("inexistente")));
        fs::remove_dir_all(&base).unwrap();
    }
}
