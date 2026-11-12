# Mensagens de origem em inglês; traduções em <idioma>.po (ver LINGUAS).
#   meson compile -C builddir cascata-pot        gera o modelo po/cascata.pot
#   meson compile -C builddir cascata-update-po  atualiza os .po
i18n.gettext(
  meson.project_name(),
  args: [
    '--from-code=UTF-8',
    '--keyword=gettext',
    '--keyword=ngettext:1,2',
    '--add-comments=TRANSLATORS:',
    '--package-name=' + meson.project_name(),
    '--package-version=' + meson.project_version(),
  ],
)
