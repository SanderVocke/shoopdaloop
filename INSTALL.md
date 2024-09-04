
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

To build from source, ensure the build dependencies for your OS are installed. The dependencies for each supported platform can be found in `distribution/dependencies` (also see the CI workflows in `.github/workflows` for practical examples).

Make sure all the subrepositories are checked out (`git submodule init; git submodule update`).

### Building from source

ShoopDaLoop's build is governed by Cargo. To build, simply run `cargo build` in the root directory. This generates `target/.../shoopdaloop(.exe)` and `target/.../shoopdaloop_dev(.exe)`. `shoopdaloop_dev` can be directly used to run within the source tree. For installation options, see below.

Be aware that a complex combination of build systems is hidden behind this cargo build front-end. It involves C/C++ compilers, `cmake`, `meson`, Python `setuptools`, code generation and more. I am working on reducing the dependencies and tools involved, but YMMV in having to install additional tools to run the build successfully.

Until this is all more stable and simple, the golden reference for how to build ShoopDaLoop is in the GitHub Actions build scripts included in the repo (`.github/workflows/...`).

### Building redistributable binaries

After building ShoopDaLoop in-tree as described above, there are several redistributable options to build. The in-tree build produces an executable called `package` which can help build these packages as follows:

#### AppDir / AppImage (Linux)

```
package
  build-appimage
  --executable=target/debug/shoopdaloop
  --dev-executable=target/debug/shoopdaloop_dev
  --appimagetool=/path/to/appimagetool
  [--include-tests]
  --output=ShoopDaLoop.AppImage
```

### Editable development build

Since ShoopDaLoop partly consists of interpreted / JIT-compiled scripts (in Lua, Python and QML), for a large part developers can make changes without having to rebuild.

In order to run ShoopDaLoop in "editable" mode (using the scripts in the repository as opposed to installing them into the system), simply run the `shoopdaloop_dev` executable compiled by Cargo instead of the regular `shoopdaloop` one.

`shoopdaloop_dev` is also the only executable that works (i.e. is able to find all of its files) without installing the software.