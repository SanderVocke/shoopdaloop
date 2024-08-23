# This script is used to find the installed location of ShoopDaLoop.

import os
import shoop_app_info

script_dir = os.path.dirname(__file__)
installed_dir = shoop_app_info.lib_dir

if not installed_dir:
    raise Exception("lib_dir not set from host code")

# Installation dir is where ShoopDaLoop is installed. In the case of
# an editable installation, generated files and back-end libs can be found here.
def installation_dir():
    return installed_dir

# Scripts dir is where the Python scripts reside. In a normal installation this
# is the same place as the installation dir, but in an editable installation
# they might be somewhere else and a distinction needs to be made.
def scripts_dir():
    return os.path.join(script_dir, '..')

# Dynlibs_dir is the path where we can find dynamic back-end libraries.
def dynlibs_dir():
    return installation_dir()

# Resources directory.
def resource_dir():
    return os.path.join(installation_dir(), 'resources')