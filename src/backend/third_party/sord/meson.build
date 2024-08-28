# Copyright 2021-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

project('sord', ['c'],
        version: '0.16.14',
        license: 'ISC',
        meson_version: '>= 0.56.0',
        default_options: [
          'b_ndebug=if-release',
          'buildtype=release',
          'c_std=c99',
        ])

sord_src_root = meson.current_source_dir()
major_version = meson.project_version().split('.')[0]
version_suffix = '-@0@'.format(major_version)
versioned_name = 'sord' + version_suffix

#######################
# Compilers and Flags #
#######################

# Required tools
pkg = import('pkgconfig')
cc = meson.get_compiler('c')

# Set global warning flags
if get_option('strict') and not meson.is_subproject()
  subdir('meson/warnings')
endif
subdir('meson/suppressions')

# Build as C++ with MSVC
if cc.get_id() == 'msvc'
  add_project_arguments(['/TP'], language: ['c'])
endif

################
# Dependencies #
################

m_dep = cc.find_library('m', required: false)

serd_dep = dependency('serd-0',
                      version: '>= 0.30.10',
                      fallback: ['serd', 'serd_dep'])

###########
# Library #
###########

c_headers = files('include/sord/sord.h')
cpp_headers = files('include/sord/sordmm.hpp')

sources = files(
  'src/sord.c',
  'src/syntax.c',
)

# Set appropriate arguments for building against the library type
extra_c_args = []
subdir('meson/library')
if get_option('default_library') == 'static'
  extra_c_args = ['-DSORD_STATIC']
endif

# Build shared and/or static library
libsord = library(
  meson.project_name() + library_suffix,
  sources,
  c_args: c_suppressions + extra_c_args + [
    '-DSORD_INTERNAL',
    '-DSORD_VERSION="@0@"'.format(meson.project_version()),
  ],
  dependencies: [m_dep, serd_dep],
  gnu_symbol_visibility: 'hidden',
  include_directories: include_directories('include', 'src'),
  install: true,
  version: meson.project_version(),
)

# Declare dependency for internal meson dependants
sord_dep = declare_dependency(
  compile_args: extra_c_args,
  dependencies: [m_dep, serd_dep],
  include_directories: include_directories('include'),
  link_with: libsord,
)

# Generage pkg-config file for external dependants
pkg.generate(
  libsord,
  description: 'Lightweight C library for storing RDF in memory',
  extra_cflags: extra_c_args,
  filebase: versioned_name,
  name: 'Sord',
  subdirs: [versioned_name],
  version: meson.project_version(),
)

# Install headers to a versioned include directory
install_headers(c_headers, subdir: versioned_name / 'sord')
install_headers(cpp_headers, subdir: versioned_name / 'sord')

#########
# Tools #
#########

# Build sordi command line utility
if not get_option('tools').disabled()
  sordi = executable('sordi',
                     files('src/sordi.c'),
                     c_args: c_suppressions,
                     install: true,
                     dependencies: sord_dep)

  meson.override_find_program('sordi', sordi)

  pcre_dep = dependency('libpcre', required: false)

  if pcre_dep.found()
    sord_validate = executable('sord_validate',
                               files('src/sord_validate.c'),
                               c_args: c_suppressions,
                               install: true,
                               dependencies: [sord_dep, pcre_dep])

    meson.override_find_program('sord_validate', sord_validate)
  endif

  if not get_option('docs').disabled()
    install_man(files('doc/sordi.1'))
    install_man(files('doc/sord_validate.1'))
  endif
endif

if not get_option('docs').disabled()
  subdir('doc')
endif

if not get_option('tests').disabled()
  subdir('test')
endif

if not meson.is_subproject()
  summary('Tests', not get_option('tests').disabled(), bool_yn: true)
  summary('Utilities', not get_option('tools').disabled(), bool_yn: true)

  summary('Install prefix', get_option('prefix'))

  summary('Headers', get_option('prefix') / get_option('includedir'))
  summary('Libraries', get_option('prefix') / get_option('libdir'))

  if not get_option('tools').disabled()
    summary('Executables', get_option('prefix') / get_option('bindir'))
    summary('Man pages', get_option('prefix') / get_option('mandir'))
  endif
endif
