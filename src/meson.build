global_conf = configuration_data()
global_conf.set_quoted('APP_ID', application_id)
global_conf.set_quoted('PKGDATADIR', pkgdatadir)
global_conf.set_quoted('PROFILE', profile)
global_conf.set_quoted('VERSION', version + version_suffix)
global_conf.set_quoted('GETTEXT_PACKAGE', gettext_package)
global_conf.set_quoted('LOCALEDIR', localedir)
config = configure_file(
  input: 'config.rs.in',
  output: 'config.rs',
  configuration: global_conf
)
# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.build_root() / 'src' / 'config.rs',
  meson.source_root() / 'src' / 'config.rs',
  check: true
)

rust_sources = files(
  'backend/mod.rs',
  'backend/pipeline_builder.rs',
  'backend/recorder.rs',
  'backend/recorder_controller.rs',
  'backend/screencast_portal.rs',
  'backend/settings.rs',
  'backend/timer.rs',
  'widgets/area_selector.rs',
  'widgets/main_window.rs',
  'widgets/mod.rs',
  'application.rs',
  'config.rs',
  'data_types.rs',
  'error.rs',
  'main.rs',
  'pactl.rs',
  'utils.rs',
)

sources = [rust_sources, cargo_sources]

custom_target(
  'cargo-build',
  build_by_default: true,
  input: sources,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depends: resources,
  command: [
    cargo_script,
    meson.build_root(),
    meson.source_root(),
    '@OUTPUT@',
    profile,
    meson.project_name(),
  ]
)

cargo_target_dir = meson.build_root() / 'target'
cargo_home = meson.build_root() / 'cargo-home'
manifest_path = meson.source_root() / 'Cargo.toml'

test (
  'cargo-test',
  cargo,
  args: [
    'test',
    '--manifest-path=@0@'.format(manifest_path),
    '--target-dir=@0@'.format(cargo_target_dir),
    '--',
    '--nocapture',
  ],
  env: [
    'CARGO_HOME=@0@'.format(cargo_home),
    'PATH=/app/bin:/usr/bin:/usr/lib/sdk/rust-stable/bin',
  ],
  timeout: 300, # give cargo more time
)

test (
  'cargo-clippy',
  cargo,
  args: [
    'clippy',
    '--manifest-path=@0@'.format(manifest_path),
    '--target-dir=@0@'.format(cargo_target_dir),
  ],
  env: [
    'CARGO_HOME=@0@'.format(cargo_home),
    'PATH=/app/bin:/usr/bin:/usr/lib/sdk/rust-stable/bin',
  ],
  timeout: 300, # give cargo more time
)
