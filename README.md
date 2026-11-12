# Cascata

**Baixe vídeos e áudios da internet, direto do GNOME.**

O Cascata é uma interface gráfica para o [yt-dlp](https://github.com/yt-dlp/yt-dlp)
e o [FFmpeg](https://ffmpeg.org), escrita em Rust com GTK 4 e libadwaita
([gtk-rs](https://gtk-rs.org)). Cole o endereço, escolha o formato e a
qualidade, e o arquivo é salvo na pasta escolhida.

- Vídeo em MP4, com resolução máxima à escolha (de 360p a 4K)
- Somente áudio, convertido para MP3
- Playlists inteiras
- Progresso, velocidade e tempo restante; cancelamento a qualquer momento
- Se o yt-dlp ou o FFmpeg não estiverem instalados, o app oferece instalar
  (Fedora e Ubuntu)
- Interface em português (Brasil) e inglês, conforme o idioma do sistema

## Como a instalação das dependências funciona

Ao abrir, o Cascata procura `yt-dlp` e `ffmpeg` no `PATH`. Se faltar algum,
aparece um aviso com o botão **Instalar**. O app mostra o comando exato e,
após a confirmação, executa-o via `pkexec` (a senha é pedida pelo próprio
sistema):

| Sistema | Comando |
| --- | --- |
| Fedora | `dnf install -y yt-dlp ffmpeg-free` |
| Ubuntu / Debian | `apt-get update && apt-get install -y yt-dlp ffmpeg` |

Só é instalado o que estiver faltando. Em outras distribuições o aviso pede a
instalação manual.

> No Ubuntu, o `yt-dlp` do repositório costuma ser antigo e pode falhar com o
> YouTube. Se isso acontecer, instale a versão atual (por exemplo
> `pipx install yt-dlp`); o Cascata usa o que estiver no `PATH`.

## Compilar

Requer Rust 1.92 ou mais novo, GTK ≥ 4.12 e libadwaita ≥ 1.5.

### Fedora

```bash
sudo dnf install rust cargo meson ninja-build gcc gtk4-devel libadwaita-devel \
                 desktop-file-utils appstream gettext
```

### Ubuntu (24.04 ou mais novo)

```bash
sudo apt install build-essential meson ninja-build libgtk-4-dev libadwaita-1-dev \
                 desktop-file-utils appstream gettext curl
# O Rust do repositório do Ubuntu 24.04 é antigo demais; use o rustup:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Executar durante o desenvolvimento

```bash
cargo run
```

### Instalar no sistema (Meson)

```bash
meson setup builddir --buildtype=release
meson compile -C builddir
meson test -C builddir
sudo meson install -C builddir
```

### GNOME Builder

Abra a pasta do projeto no Builder (**Abrir projeto…**). O `meson.build` é
detectado automaticamente; escolha o runtime **Host** (ou um toolbox com os
pacotes acima) e use **Executar**. O app precisa rodar fora do sandbox do
Flatpak para enxergar o `yt-dlp`/`ffmpeg` do sistema e para instalar pacotes.

## Idiomas

A interface usa o gettext. As mensagens no código estão em inglês, que é o
idioma padrão; a tradução para o português do Brasil fica em `po/pt_BR.po`.
O idioma segue o do sistema. Para testar outro:

```bash
LANGUAGE=pt_BR cascata
LANGUAGE=en_US cascata
```

As traduções são instaladas pelo `meson install`. Com `cargo run` o app
aparece em inglês, a menos que você compile o catálogo e aponte para ele:

```bash
mkdir -p target/locale/pt_BR/LC_MESSAGES
msgfmt -o target/locale/pt_BR/LC_MESSAGES/cascata.mo po/pt_BR.po
CASCATA_LOCALEDIR=target/locale LANGUAGE=pt_BR cargo run
```

Depois de alterar textos no código, atualize os catálogos:

```bash
meson compile -C builddir cascata-update-po
```

Para adicionar um idioma, inclua o código dele em `po/LINGUAS` e crie o
`po/<idioma>.po` a partir do modelo (`meson compile -C builddir cascata-pot`).

## Testes

```bash
cargo test

# Ponta a ponta, com download real (usa a rede):
CASCATA_TEST_URL="https://…" cargo test real_download -- --ignored --nocapture
CASCATA_TEST_URL="https://…" CASCATA_TEST_KIND=audio \
    cargo test real_download -- --ignored --nocapture
```

## Estrutura

```
src/main.rs        aplicação (adw::Application)
src/window.rs      janela e interface (libadwaita)
src/about.rs       diálogo "Sobre" e informações de depuração
src/i18n.rs        configuração do gettext
src/downloader.rs  executa o yt-dlp e traduz a saída em eventos
src/deps.rs        detecção e instalação do yt-dlp e do FFmpeg
src/config.rs      ID, nome e endereços do projeto
data/              .desktop, metainfo (AppStream) e ícone
po/                traduções (gettext)
```

O ID do aplicativo é `io.github.ovitorvalente.Cascata`.

## Aviso

Baixe apenas conteúdo próprio, de licença livre ou com permissão do autor.
Baixar vídeos pode violar os termos de uso de alguns sites.

## Licença

MIT — veja [LICENSE](LICENSE).
