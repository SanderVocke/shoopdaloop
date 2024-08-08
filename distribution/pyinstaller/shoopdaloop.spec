# -*- mode: python ; coding: utf-8 -*-
import shoopdaloop
import os
import re
import platform

from shoopdaloop.lib.directories import *

# Always use the system-installed shoopdaloop
shoopdaloop_install_path = os.path.dirname(shoopdaloop.__file__)

a = Analysis(
    [f'{shoopdaloop_install_path}/__main__.py'],
    pathex=[],
    binaries=[],
    datas=[],
    hiddenimports=['shoopdaloop'],
    hookspath=['.'],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False
)

excluded_binaries = [
    'libjack.so.*',    # JACK client library should always come from the system
    'libxkbcommon.*',  # TODO: figure out root-cause, having libxkbcommon but not libxkbcommon-x11 in the package caused a segfault on Arch
    'libstdc\+\+.*',   # MESA won't roll with old c++ std libraries. See discussion @ https://github.com/pyinstaller/pyinstaller/issues/6993
    'libQt6.*',        # Qt6 libraries will already be included through the PySide6 hook.
]
added_binaries = [
    'libcrypt.so.1',  # This is in Pyinstaller's default blacklist apparently. Arch does need it
]
a.binaries = TOC([x for x in a.binaries if len([e for e in excluded_binaries if re.match(e, x[0])]) == 0])

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='shoopdaloop',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='shoopdaloop',
)
opts = {
    'windowed': True
}

if platform.system() == "Darwin":
    app = BUNDLE(
        coll,
        name='shoopdaloop.app',
        icon=os.path.join(resource_dir(), 'iconset', 'icon.icns'),
        bundle_identifier=None,
        version='0.0.1',
        info_plist={
            'NSPrincipalClass': 'NSApplication',
            'NSAppleScriptEnabled': False,
            'CFBundleDocumentTypes': [
                {
                    'CFBundleTypeName': 'ShoopDaLoop Session',
                    'CFBundleTypeRole': 'Editor',
                    'LSItemContentTypes': ['com.shoopdaloop.session'],
                    'LSHandlerRank': 'Owner'
                }
            ]
        },
    )
