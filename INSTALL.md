
# Installation

ShoopDaLoop is built and distributed as a Python package. Installation options are still quite limited as this is in pre-alpha development.

There are no official binaries or packages yet. However, you can probably grab a recent .whl package from a continuous integration build. Navigate to the 'Actions' tab on GitHub, select the 'Build And Test' workflow, choose the build you want to pick the wheel from and download one of the wheels there. Of course, YMMV.

Be aware that a recent version of Qt/PySide is needed in any case (>=6.4.2). This is not yet offered by most distros, although you may have luck by using PySide from the PyPi repositories.

## From source

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
