# ESPERUST

An experimental S4 League recreation written in Rust with Bevy. Entirely by AI.

## Credit
Vivy for the supplied 1162 unpacked client.
Everyone that contributed to the S4 RE scene, including reversed server files, scn tooling, etc. As this wouldn't have been possible without them.
While this project was constricted in building everything in-house. I cannot guarantee it did. Plus I'm sure the models were pre-trained on public repos anyway.

## Download

Extract a compiled build from this repository's **Releases** page and run
`esperust` (`esperust.exe` on Windows). Python and importer libraries are included;
keep the extracted folders beside the executable.

## Build

Run these commands from the repository root. The build scripts check dependencies,
show their installation plan, and ask before installing anything. They set up
Python requirements in `.venv`, the native importer libraries, and Rust 1.93.1,
then build a release. An internet connection is needed for missing dependencies.

### Linux

```sh
./build-linux.sh
./target/release/esperust
```

The script supports Ubuntu/Debian, Arch and Fedora. Use `--dry-run` to see its plan,
or `--skip-system` if you manage system packages yourself. Install the Vulkan
driver for your GPU through your distribution.

For a manual build, install [Rust with rustup](https://rustup.rs/) and the packages
for your distribution:

#### Ubuntu / Debian

```sh
sudo apt install build-essential pkg-config curl python3 python3-venv python3-pip python3-tk libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev libssl-dev liblzo2-dev libvulkan1
```

#### Arch

On an up-to-date Arch system:

```sh
sudo pacman -S --needed base-devel pkgconf curl python python-pip tk libx11 alsa-lib systemd-libs libxcursor libxrandr libxi wayland libxkbcommon libxkbcommon-x11 openssl lzo vulkan-icd-loader
```

Then, for either distribution:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r tools/requirements.txt
.venv/bin/python tools/setup_importer.py
cargo build --release --locked
./target/release/esperust
```

For other distributions, see [Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/v0.18.1/docs/linux_dependencies.md).

### Windows

In PowerShell or Command Prompt on 64-bit Windows:

```powershell
.\build-windows.bat
.\target\release\esperust.exe
```

The script checks `py`, `python`, `python3`, versioned commands and registered
Python installations. If needed, it installs Python, Git and Visual Studio Build
Tools with C++ and a Windows SDK. OpenSSL and LZO are built locally with vcpkg.
Windows may request administrator approval; if an installer requests a restart,
restart and run the script again. `build-windows.bat -DryRun` only shows the plan.

#### Manual setup

Install these first:

- [Python 3.11+ (64-bit)](https://www.python.org/downloads/windows/), including pip and Tcl/Tk.
- [Git for Windows](https://git-scm.com/downloads/win).
- [Rust with rustup](https://rustup.rs/), using the MSVC toolchain.
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), with **Desktop development with C++** and a Windows SDK.

Open **Developer PowerShell for Visual Studio** after installation and change to
the repository folder. These commands use `py -3`; substitute your Python 3.11+
command if you do not have the Python launcher.

```powershell
py -3 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r tools\requirements.txt
git clone --depth 1 https://github.com/microsoft/vcpkg.git .build\vcpkg
.\.build\vcpkg\bootstrap-vcpkg.bat -disableMetrics
.\.build\vcpkg\vcpkg.exe install openssl:x64-windows lzo:x64-windows --disable-metrics
.\.venv\Scripts\python.exe tools\setup_importer.py --library-dir .build\vcpkg\installed\x64-windows\bin
rustup toolchain install 1.93.1-x86_64-pc-windows-msvc --profile minimal
cargo +1.93.1-x86_64-pc-windows-msvc build --release --locked
.\target\release\esperust.exe
```

If `.build\vcpkg` already exists from a previous setup, skip the clone command.

### macOS

Install Apple's command-line tools and finish the installer before continuing:

```sh
xcode-select --install
```

Install [Homebrew](https://brew.sh/) if needed, then:

```sh
brew install rustup python@3.12 python-tk@3.12 openssl@3 lzo pkgconf
export PATH="$(brew --prefix rustup)/bin:$PATH"
rustup toolchain install 1.93.1 --profile minimal
"$(brew --prefix python@3.12)/bin/python3.12" -m venv .venv
.venv/bin/python -m pip install -r tools/requirements.txt
.venv/bin/python tools/setup_importer.py --library-dir "$(brew --prefix openssl@3)/lib" --library-dir "$(brew --prefix lzo)/lib"
cargo +1.93.1 build --release --locked
./target/release/esperust
```

Bevy uses Metal on macOS. macOS builds and gameplay have not been tested.

### Assets

On first launch, select your client folder containing `_resources/`. Include its
matching `resource.s4hd` to select the correct revisions when needed. You can also
pass `--resources /path/to/client` when launching the game.

Source builds require Python 3.11+, Pillow, OpenSSL with SEED support, and
libLZO2. The build scripts install these; releases include them.

Converted assets are saved in `assets/` and reused on later launches.
