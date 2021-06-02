pkgdatadir = join_paths(get_option('prefix'), get_option('datadir'), meson.project_name())
moduledir = join_paths(pkgdatadir, 'kooha')
gnome = import('gnome')

python = import('python')

conf = configuration_data()
conf.set('PYTHON', python.find_installation('python3').path())
conf.set('VERSION', meson.project_version())
conf.set('localedir', join_paths(get_option('prefix'), get_option('localedir')))
conf.set('pkgdatadir', pkgdatadir)

configure_file(
  input: 'kooha.in',
  output: 'kooha',
  configuration: conf,
  install: true,
  install_dir: get_option('bindir')
)

kooha_sources = [
  '__init__.py',
  'main.py',
  'window.py',
]

install_data(kooha_sources, install_dir: moduledir)
install_subdir('backend', install_dir: moduledir)
