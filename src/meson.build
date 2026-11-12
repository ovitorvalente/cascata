cargo = find_program('cargo')

cargo_options = [
  '--manifest-path', meson.project_source_root() / 'Cargo.toml',
  '--target-dir', meson.project_build_root() / 'src',
]

if get_option('buildtype') == 'release' or get_option('buildtype') == 'plain'
  cargo_options += ['--release']
  rust_target = 'release'
else
  rust_target = 'debug'
endif

# O Meson delega a compilação ao Cargo e copia o binário resultante.
custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    cargo, 'build', cargo_options,
    '&&',
    'cp', 'src' / rust_target / meson.project_name(), '@OUTPUT@',
  ],
)

test(
  'cargo-test',
  cargo,
  args: ['test', cargo_options],
  timeout: 600,
)
