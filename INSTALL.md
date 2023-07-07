
# Installation

## From a distro package

There are no official packages in any distro's repositories yet, but there is a preview in this repo that can be used already:

- For Arch, a PKGBUILD can be generated using `distribution/packaging/arch/PKGBUILD.git.generate.sh` (pipe the output into a PKGBUILD file). It can then be installed using `makepkg`. Be aware that this PKGBUILD will again pull in this Git repo and build it.

## From source

ShoopDaLoop is built and distributed as a Python package. Installation options are still quite limited as this is in pre-alpha development.

To build from source, ensure the build dependencies for your OS are installed. The dependencies for each supported platform can be found in `distribution/dependencies` (also see the CI workflows in `.github/workflows` for practical examples).

It may also be possible to skip installing some of the Python dependencies and let them be installed later by `pip` (YMMV).

### Building the .whl from source

The regular way to install ShoopDaLoop is to build a Python .whl package and install it. Usually this should happen in a virtual environment. 

Building the .whl:

```
python3 -m build --no-isolation -wheel .
```

Note that ShoopDaLoop does not support generating a source distribution, hence the `--wheel` argument to build the .whl directly.

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

This will install into `./.py-build-cmake-cache/editable` with symlinks to the Python and QML source files in this repo. It will also install a link into the system such that `shoopdaloop` can be run system-wide. Note that the installation command should be repeated if anything changes which affects the CMake build (C / C++ files, package metadata version/description).
