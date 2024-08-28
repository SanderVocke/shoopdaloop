# Copyright 2021-2022 David Robillard <d@drobilla.net>
# SPDX-License-Identifier: 0BSD OR ISC

project('sratom', ['c'],
        version: '0.6.14',
        license: 'ISC',
        meson_version: '>= 0.56.0',
        default_options: [
          'b_ndebug=if-release',
          'buildtype=release',
          'c_std=c99',
        ])

sratom_src_root = meson.current_source_dir()
major_version = meson.project_version().split('.')[0]
version_suffix = '-@0@'.format(major_version)
versioned_name = 'sratom' + version_suffix

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

################
# Dependencies #
################

m_dep = cc.find_library('m', required: false)

lv2_dep = dependency('lv2',
                     version: '>= 1.18.4',
                     fallback: ['lv2', 'lv2_dep'])

serd_dep = dependency('serd-0',
                      version: '>= 0.30.10',
                      fallback: ['serd', 'serd_dep'])

sord_dep = dependency('sord-0',
                      version: '>= 0.16.10',
                      fallback: ['sord', 'sord_dep'])

###########
# Library #
###########

include_dirs = include_directories('include')
c_headers = files('include/sratom/sratom.h')
sources = files('src/sratom.c')

# Set appropriate arguments for building against the library type
extra_c_args = []
subdir('meson/library')
if get_option('default_library') == 'static'
  extra_c_args = ['-DSRATOM_STATIC']
endif

# Build shared and/or static library
libsratom = library(
  meson.project_name() + library_suffix,
  sources,
  c_args: c_suppressions + extra_c_args + ['-DSRATOM_INTERNAL'],
  dependencies: [m_dep, lv2_dep, serd_dep, sord_dep],
  gnu_symbol_visibility: 'hidden',
  include_directories: include_dirs,
  install: true,
  version: meson.project_version(),
)

# Declare dependency for internal meson dependants
sratom_dep = declare_dependency(
  compile_args: extra_c_args,
  dependencies: [m_dep, lv2_dep, serd_dep, sord_dep],
  include_directories: include_dirs,
  link_with: libsratom
)

# Generage pkg-config file for external dependants
pkg.generate(
  libsratom,
  description: 'Small library for serializing LV2 atoms',
  extra_cflags: extra_c_args,
  filebase: versioned_name,
  name: 'Sratom',
  subdirs: [versioned_name],
  version: meson.project_version(),
)

# Install header to a versioned include directory
install_headers(c_headers, subdir: versioned_name / 'sratom')

###########
# Support #
###########

if not get_option('tests').disabled()
  subdir('test')
endif

if not get_option('docs').disabled()
  subdir('doc')
endif

if not meson.is_subproject()
  summary('Tests', not get_option('tests').disabled(), bool_yn: true)

  summary('Install prefix', get_option('prefix'))

  summary('Headers', get_option('prefix') / get_option('includedir'))
  summary('Libraries', get_option('prefix') / get_option('libdir'))
endif
