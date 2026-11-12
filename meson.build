project(
  'cascata',
  'rust',
  version: '0.1.0',
  license: 'MIT',
  meson_version: '>= 0.62.0',
)

gnome = import('gnome')
i18n = import('i18n')

app_id = 'io.github.ovitorvalente.Cascata'

# Bibliotecas de sistema exigidas pelas crates gtk4 e libadwaita.
dependency('gtk4', version: '>= 4.12')
dependency('libadwaita-1', version: '>= 1.5')

subdir('data')
subdir('po')
subdir('src')

gnome.post_install(
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
