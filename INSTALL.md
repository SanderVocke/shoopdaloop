
# Installation

There are multiple ways to install ShoopDaLoop: from source or from pre-release binaries and packages. For binaries and packages, check the latest release page.

# Packages

Automatically built packages include:

- Python wheels for Linux/Windows/Mac (`*.whl`). These can be installed with `pip`. The advantage is that they leverage packages installed in your Python distribution (which could pull them in from `pip` or depend on packages already installed via your OS).
- Portable runnable folders (`portable_...`). These include all dependencies in one folder, making them large but reliable. They can be extracted and run directly.
- Packages per platform:
   - For Linux, there are `rpm`, `deb` and `pacman` binary packages. These are "fat" packages which install `shoopdaloop` into `/opt` along with all dependencies (many of which are sourced from PyPi). If you want to utilize your distro's libraries instead, building from source or using a wheel is needed.
   - For Windows, the package is a `.exe` installer.
   - For Mac, there is a `.dmg` disk image package. Double-click it to mount the image. The app can be started directly from the mounted volume, or copied to the Applications folder for a permanent installation. If MacOS complains that you cannot start due to not being able to check for malicious software, you can go to your Mac's privacy settings screen and make an exception at the bottom.
 
Note that the latest Python wheel packages can also be installed directly from PyPi: `python -m pip install shoopdaloop`.

> :warning: **In ALL cases: install at your own risk. I do not take responsibility for any harm done to your system.** Be aware that these wheels and packages include code from not just this repository, but also:
>  - several GitHub linked repo's;
>  - packages pulled in during the CI build from PyPi repositories;
>  - in case of "fat" packages: system libraries duplicated from the build distro;
> 
> None of the above are created, controlled or thoroughly audited by me. For maximum control, you can build your own packages and dependencies from source.

There are future plans for an AUR package for Arch, which links against the distro's libraries. For other Linuxes, I do not plan to create and maintain additional packages.

## From source

To build from source, ensure the build dependencies for your OS are installed. The dependencies for each supported platform can be found in `distribution/dependencies` (also see the CI workflows in `.github/workflows` for practical examples).

It may also be possible to skip installing some of the Python dependencies and let them be installed later by `pip` (YMMV).

Make sure all the subrepositories are checked out (`git submodule init; git submodule update`).

### Building the .whl from source

The regular way to install ShoopDaLoop is to build a Python .whl package and install it. Usually this should happen in a virtual environment. 

Building the .whl:

```
python -m build --wheel .
```

Note that ShoopDaLoop does not support generating a source distribution, hence the `--wheel` argument to build the .whl directly.

Also note that depending on OS some extra preparation may be needed to get the compilers and tools all configured correctly. Work is needed to document these steps properly, but for now I would point anyone to the Github Actions CI scripts in the .github folder - in particular the `prepare_build_...` actions that set up environments for the build per OS.

### Installing the .whl (regular)

Assuming you are in a virtualenv (probably best to use `--system-site-packages` to prevent duplication):

```
pip install dist/shoopdaloop-*.whl
```

You can also substitute `pipx` for `pip` if you want the virtualenv part to be managed automatically.

### Editable development install

A develpment install can be done using:

```
pip install -e . --no-build-isolation
```

This will install into `./.py-build-cmake-cache/editable` with symlinks to the Python and QML source files in this repo. It will also install a link into the system such that `shoopdaloop` can be run system-wide.

In other words, it installs binaries into wherever Python libraries are normally installed, while keeping interpreted scripts (Python, QML) as symlinks, allowing for quick changes.

Note that the installation command should be repeated if anything changes which affects the CMake build (C / C++ files, package metadata version/description).

### Changing CMake build settings

Build settings are controlled by `pyproject.toml`, including CMake build settings. To change some settings locally, an override file can be used to change only those settings needed. It should be stored alongside `pyproject.toml` and its filename should be `py-build-cmake.local.toml`. Some examples are stored in the `overrides` folder and can be copied directly, e.g. to do a debug or asan build.
