{
  description = "ShoopDaLoop development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      rustToolchain = pkgs.rust-bin.stable.latest.default.override {
        extensions = [
          "llvm-tools-preview"
          "rust-src"
          "rustfmt"
        ];
        targets = [ "wasm32-unknown-unknown" ];
      };

      python = pkgs.python3.withPackages (pythonPackages: [
        pythonPackages.pip
        pythonPackages.selenium
        pythonPackages.sphinx
      ]);

      buildLibraries = with pkgs; [
        alsa-lib
        file
        fluidsynth
        gtk2
        gtk3
        libjack2
        liblo
        libpulseaudio
        libsndfile
        rubberband
        libxcb
        libxkbcommon
        wayland
        xcbutilimage
        xcbutilkeysyms
        xcbutilrenderutil
        xcbutilwm
        libx11
        libxcursor
        libxi
        libxrandr
      ];

      runtimeLibraries = buildLibraries ++ (with pkgs; [ libglvnd ]);
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          rustToolchain
          cargo-llvm-cov
          cargo-nextest
          curl
          git
          gnumake
          nodejs_22
          patchelf
          pkg-config
          python
          trunk
          wasm-pack
          xvfb-run
        ];

        buildInputs = buildLibraries;

        SHOOP_CARLA_NATIVE_LIBRARY = "${pkgs.carla}/lib/carla/libcarla_native-plugin.so";
        SHOOP_CARLA_RESOURCE_DIR = "${pkgs.carla}/share/carla/resources";
        shellHook = ''
          jack_provider="''${SHOOP_JACK_PROVIDER_OVERRIDE:-auto}"

          if [ "$jack_provider" = auto ]; then
            native_jack_active=false
            for comm_file in /proc/[0-9]*/comm; do
              if IFS= read -r process_name < "$comm_file"; then
                case "$process_name" in
                  jackd|jackdmp|jackdbus)
                    native_jack_active=true
                    break
                    ;;
                esac
              fi
            done

            if [ "$native_jack_active" = true ]; then
              jack_provider=jack
            elif command -v pw-cli >/dev/null 2>&1 \
              && pw-cli info 0 >/dev/null 2>&1; then
              jack_provider=pipewire
            else
              jack_provider=host
              host_has_jack=false
              old_ifs="$IFS"
              IFS=:
              for lib_dir in ''${LD_LIBRARY_PATH:-}; do
                if [ -e "$lib_dir/libjack.so.0" ]; then
                  host_has_jack=true
                  break
                fi
              done
              IFS="$old_ifs"

              if [ "$host_has_jack" = false ]; then
                jack_provider=jack
              fi
            fi
          fi

          case "$jack_provider" in
            jack)
              jack_library_path="${pkgs.libjack2}/lib"
              ;;
            pipewire)
              jack_library_path="${pkgs.pipewire.jack}/lib"
              ;;
            host)
              jack_library_path=""
              ;;
            *)
              echo "Unknown SHOOP_JACK_PROVIDER_OVERRIDE '$jack_provider'; expected auto, jack, or pipewire" >&2
              jack_provider=jack
              jack_library_path="${pkgs.libjack2}/lib"
              ;;
          esac

          export SHOOP_JACK_PROVIDER="$jack_provider"
          export LD_LIBRARY_PATH="$jack_library_path''${jack_library_path:+:}${pkgs.lib.makeLibraryPath runtimeLibraries}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          export LIBGL_DRIVERS_PATH="/run/opengl-driver/lib/dri''${LIBGL_DRIVERS_PATH:+:$LIBGL_DRIVERS_PATH}"
          export GBM_BACKENDS_PATH="/run/opengl-driver/lib/gbm''${GBM_BACKENDS_PATH:+:$GBM_BACKENDS_PATH}"
          export __EGL_VENDOR_LIBRARY_DIRS="/run/opengl-driver/share/glvnd/egl_vendor.d''${__EGL_VENDOR_LIBRARY_DIRS:+:$__EGL_VENDOR_LIBRARY_DIRS}"
        '';
      };
    };
}
