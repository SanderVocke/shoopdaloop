
# Installation

There are multiple ways to install ShoopDaLoop: from source or from pre-release binaries and packages. For binaries and packages, check the latest release page.

# From binaries

ShoopDaLoop is distributed either as a portable folder or an OS-specific installable artifact. I am not aware of any maintained packages in distro repositories at the moment (let me know if you want to make one!).

- Portable runnable folders (`portable_...`). These include all dependencies in one folder, making them large but easy to use, with hardly any exernal dependencies. They can be extracted and run directly.
- Installables per platform:
   - For Linux, `shoopdaloop` is released as an AppImage.
   - For Windows, the package is a `.exe` installer.
   - For Mac, there is a `.dmg` disk image package. Double-click it to mount the image. The app can be started directly from the mounted volume, or copied to the Applications folder for a permanent installation. If MacOS complains that you cannot start due to not being able to check for malicious software, you can go to your Mac's privacy settings screen and make an exception at the bottom.

> :warning: **In ALL cases: install at your own risk. I do not take responsibility for any harm done to your system.** Be aware that these wheels and packages include code from not just this repository, but also:
>  - several GitHub linked repo's;
>  - packages pulled in during the CI build from PyPi repositories;
>  - in case of "fat" packages: system libraries duplicated from the build distro;
>
> None of the above are created, controlled or thoroughly audited by me. For maximum control, you can build your own packages and dependencies from source.

There are future plans for an AUR package for Arch, which links against the distro's libraries. For other Linuxes, I do not plan to create and maintain additional packages.

## From source

### Dependencies

To build from source, ensure the build dependencies for your OS are installed. The dependencies for each supported platform can be found in `distribution/dependencies` (also see the CI workflows in `.github/workflows` for practical examples).

Make sure all the subrepositories are checked out (`git submodule init; git submodule update`).

### Building from source

ShoopDaLoop's build is governed by Cargo. In principle, the build command is `cargo build [--release]`.

However, managing dependencies can get complicated:

* Rust dependencies are pulled in and built on-the-fly by Cargo.
* C++ dependencies are not pulled in but searched on your system by CMake.

For reproducible builds, a script is included which pulls C++ dependencies from `vcpkg` and builds them from source (including Qt). To run this script:

`python scripts/vcpkg_prebuild.py`

This will obviously take a long time, but when done, the folder `build/vcpkg_installed` will contain the required dependencies. There will also be a set of `build/build-env-[debug|release].[sh|ps1|elv]` scripts created, which you can source to set the environment to start the cargo build.

Alternatively, you can set up the C++ dependencies however you wish (e.g. distro packages). If things are not auto-detected you can manually set `QMAKE`.

After the Cargo build, executables can be found in `target/[debug|release]`.

### Building redistributable binaries

After building ShoopDaLoop in-tree as described above, there are several redistributable options to build. `./target/<release|debug>/package` is the command that builds these packages. See `.github/actions/build_package/action.yml` for example invocations to produce various packages.

### Editable development build

Since ShoopDaLoop partly consists of interpreted / JIT-compiled scripts (in Lua and QML), for a these parts developers can make changes without having to rebuild.

In order to run ShoopDaLoop in "editable" mode (using the scripts in the repository as opposed to installing them into the system), simply run:

- `target/[debug/release]/shoopdaloop_dev.sh` on Linux and macOS
- `target/[debug/release]/shoopdaloop_dev.bat` on Windows