# This script is used to find the installed location of ShoopDaLoop.

import os

script_dir = os.path.dirname(__file__)
installed_dir = None

try:
    # If installed in a regular fashion, there will be a version.txt relative
    # to this file.
    with open(script_dir + '/../version.txt', 'r') as f:
        installed_dir = script_dir + '/..'
except:
    # If we could not find version.txt, we are probably in an editable install
    # where it is not generated. We need to find the installation directory,
    # which is not directly possible but we can try the site folder.    
    import shoopdaloop
    try_dir = os.path.dirname(shoopdaloop.__file__)
    if os.path.exists(os.path.join(try_dir, 'version.txt')):
        installed_dir = try_dir

if not installed_dir:
    raise Exception("Unable to find version.txt in source or installation dir.")

# Installation dir is where ShoopDaLoop is installed. In the case of
# an editable installation, generated files and back-end libs can be found here.
def installation_dir():
    if not installed_dir:
        raise Exception("Unable to locate ShoopDaLoop installation")
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