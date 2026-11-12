//! Ícone na bandeja do sistema (StatusNotifierItem, via `ksni`) e a
//! extensão do GNOME Shell que o exibe.
//!
//! O GNOME não mostra ícones de bandeja por conta própria: é preciso a
//! extensão "AppIndicator and KStatusNotifierItem Support". No Ubuntu uma
//! variante dela já vem ativada; no Fedora o Cascata oferece instalá-la.

use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gio, glib};
use ksni::blocking::TrayMethods;

use crate::charts::Pixmap;
use crate::config;
use crate::i18n::fill;

const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";

const SHELL_NAME: &str = "org.gnome.Shell.Extensions";
const SHELL_PATH: &str = "/org/gnome/Shell/Extensions";

/// Extensão publicada em extensions.gnome.org.
const EXTENSION_UUID: &str = "appindicatorsupport@rgcjonas.gmail.com";
/// Variante que acompanha o Ubuntu.
const UBUNTU_EXTENSION_UUID: &str = "ubuntu-appindicators@ubuntu.com";

/// Tempo para o usuário responder ao diálogo do GNOME e a extensão baixar.
const INSTALL_TIMEOUT_MS: i32 = 5 * 60 * 1000;
const CALL_TIMEOUT_MS: i32 = 10 * 1000;

/// Pedidos feitos pelo menu da bandeja, tratados na thread principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Show,
    ShowDetails,
    CancelDownload,
    /// Mostra o último arquivo concluído no gerenciador de arquivos.
    ShowLastFile,
    Quit,
}

/// O que a bandeja mostra em um dado momento. É montado na thread
/// principal, já com os textos traduzidos e os desenhos prontos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    /// Ícone desenhado na hora; vazio = o ícone normal do aplicativo.
    pub icon: Vec<Pixmap>,
    /// Resumo de uma linha, usado na dica do ícone.
    pub summary: String,
    /// Linhas informativas do menu (título, progresso, velocidade…).
    pub lines: Vec<String>,
    /// Miniatura do gráfico de velocidade (PNG), ao lado da primeira linha.
    pub sparkline: Vec<u8>,
    pub can_cancel: bool,
    /// Nome do último arquivo concluído.
    pub last_file: Option<String>,
}

struct CascataTray {
    commands: async_channel::Sender<Command>,
    view: View,
}

impl CascataTray {
    fn send(&self, command: Command) {
        let _ = self.commands.send_blocking(command);
    }
}

fn action(label: String, command: Command) -> ksni::MenuItem<CascataTray> {
    ksni::menu::StandardItem {
        label,
        activate: Box::new(move |tray: &mut CascataTray| tray.send(command)),
        ..Default::default()
    }
    .into()
}

/// No protocolo do menu, "_" marca tecla de atalho; textos livres, como o
/// título do vídeo, precisam ter o caractere duplicado.
fn literal(label: &str) -> String {
    label.replace('_', "__")
}

impl ksni::Tray for CascataTray {
    fn id(&self) -> String {
        config::APP_ID.into()
    }

    fn title(&self) -> String {
        config::APP_NAME.into()
    }

    fn icon_name(&self) -> String {
        // Com um ícone desenhado, o nome fica vazio para ele ter prioridade.
        if self.view.icon.is_empty() {
            config::APP_ID.into()
        } else {
            String::new()
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.view
            .icon
            .iter()
            .map(|pixmap| ksni::Icon {
                width: pixmap.size,
                height: pixmap.size,
                data: pixmap.argb.clone(),
            })
            .collect()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: config::APP_NAME.into(),
            description: self.view.summary.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        use ksni::MenuItem::Separator;

        let mut items = vec![
            action(gettext("Open Cascata"), Command::Show),
            action(gettext("Download details"), Command::ShowDetails),
        ];

        if !self.view.lines.is_empty() {
            items.push(Separator);
            for (index, line) in self.view.lines.iter().enumerate() {
                items.push(
                    StandardItem {
                        label: literal(line),
                        enabled: false,
                        icon_data: if index == 0 {
                            self.view.sparkline.clone()
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    }
                    .into(),
                );
            }
        }
        if self.view.can_cancel {
            items.push(action(gettext("Cancel download"), Command::CancelDownload));
        }

        if let Some(name) = &self.view.last_file {
            items.push(Separator);
            let label = fill(
                &gettext("Show “{name}” in folder"),
                &[("name", &literal(name))],
            );
            items.push(action(label, Command::ShowLastFile));
        }

        items.push(Separator);
        items.push(action(gettext("Quit"), Command::Quit));
        items
    }
}

/// Ícone ativo na bandeja. Some ao ser descartado.
pub struct Tray {
    handle: ksni::blocking::Handle<CascataTray>,
}

impl Tray {
    /// Registra o ícone. Falha se não houver bandeja no ambiente.
    pub async fn start(commands: async_channel::Sender<Command>) -> Result<Self, String> {
        // O registro conversa com o D-Bus de forma bloqueante; fica fora da
        // thread da interface.
        let task = gio::spawn_blocking(move || {
            CascataTray {
                commands,
                view: View::default(),
            }
            .spawn()
            .map(|handle| Tray { handle })
            .map_err(|err| err.to_string())
        });
        task.await
            .unwrap_or_else(|_| Err(gettext("The tray icon could not be created.")))
    }

    /// Troca o ícone, a dica e o conteúdo do menu.
    pub fn set_view(&self, view: View) {
        self.handle.update(move |tray| tray.view = view);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // Não espera a conclusão para não travar a interface.
        let _ = self.handle.shutdown();
    }
}

/// Mensagem de um erro do D-Bus, sem o prefixo técnico "GDBus.Error:…".
fn error_text(mut error: glib::Error) -> String {
    gio::DBusError::strip_remote_error(&mut error);
    error.message().to_string()
}

async fn session_bus() -> Result<gio::DBusConnection, String> {
    gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(error_text)
}

/// Indica se algum serviço de bandeja está em execução na sessão.
pub async fn available() -> bool {
    let Ok(bus) = session_bus().await else {
        return false;
    };
    bus.call_future(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&(WATCHER_NAME,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        CALL_TIMEOUT_MS,
    )
    .await
    .ok()
    .and_then(|reply| reply.get::<(bool,)>())
    .is_some_and(|(has_owner,)| has_owner)
}

/// Espera a bandeja aparecer depois de ativar a extensão.
pub async fn wait_until_available(seconds: u32) -> bool {
    for _ in 0..seconds * 2 {
        if available().await {
            return true;
        }
        glib::timeout_future(std::time::Duration::from_millis(500)).await;
    }
    available().await
}

async fn shell_call(
    bus: &gio::DBusConnection,
    method: &str,
    uuid: &str,
    timeout_ms: i32,
) -> Result<glib::Variant, String> {
    bus.call_future(
        Some(SHELL_NAME),
        SHELL_PATH,
        SHELL_NAME,
        method,
        Some(&(uuid,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        timeout_ms,
    )
    .await
    .map_err(error_text)
}

async fn extension_installed(bus: &gio::DBusConnection, uuid: &str) -> bool {
    shell_call(bus, "GetExtensionInfo", uuid, CALL_TIMEOUT_MS)
        .await
        .is_ok_and(|reply| reply.child_value(0).n_children() > 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionOutcome {
    /// A extensão está instalada e foi ativada.
    Enabled,
    /// O usuário recusou a instalação no diálogo do GNOME.
    Cancelled,
    Failed(String),
}

/// Ativa a extensão de bandeja do GNOME Shell, instalando-a de
/// extensions.gnome.org se for preciso. A instalação é confirmada pelo
/// usuário em um diálogo do próprio GNOME.
pub async fn enable_extension() -> ExtensionOutcome {
    let bus = match session_bus().await {
        Ok(bus) => bus,
        Err(message) => return ExtensionOutcome::Failed(message),
    };

    for uuid in [UBUNTU_EXTENSION_UUID, EXTENSION_UUID] {
        if extension_installed(&bus, uuid).await {
            return match shell_call(&bus, "EnableExtension", uuid, CALL_TIMEOUT_MS).await {
                Ok(reply) if reply.get::<(bool,)>() == Some((true,)) => ExtensionOutcome::Enabled,
                Ok(_) => {
                    ExtensionOutcome::Failed(gettext("GNOME Shell could not enable the extension."))
                }
                Err(message) => ExtensionOutcome::Failed(message),
            };
        }
    }

    match shell_call(
        &bus,
        "InstallRemoteExtension",
        EXTENSION_UUID,
        INSTALL_TIMEOUT_MS,
    )
    .await
    {
        Ok(reply) => match reply.get::<(String,)>().as_ref().map(|(r,)| r.as_str()) {
            Some("successful") => ExtensionOutcome::Enabled,
            Some("cancelled") => ExtensionOutcome::Cancelled,
            other => ExtensionOutcome::Failed(other.unwrap_or_default().to_string()),
        },
        Err(message) => ExtensionOutcome::Failed(message),
    }
}
