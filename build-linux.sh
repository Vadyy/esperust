#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$project_dir"

skip_system=0
dry_run=0
for argument in "$@"; do
  case "$argument" in
    --skip-system) skip_system=1 ;;
    --dry-run) dry_run=1 ;;
    -h|--help)
      printf '%s\n' 'Usage: ./build-linux.sh [--skip-system] [--dry-run]' \
        'Checks dependencies, asks before installing, then builds a release.' \
        '--skip-system: leave system packages alone (local Rust/Python setup still runs).' \
        '--dry-run: show the plan without installing or building.'
      exit 0 ;;
    *) printf 'Unknown option: %s\n' "$argument" >&2; exit 2 ;;
  esac
done
[[ "$(uname -s)" == Linux ]] || { printf '%s\n' 'This script is for Linux.' >&2; exit 1; }

manager=''
packages=()
if command -v apt-get >/dev/null; then
  manager=apt
  packages=(build-essential pkg-config curl ca-certificates python3 python3-venv python3-pip python3-tk
    libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev libssl-dev liblzo2-dev libvulkan1)
elif command -v pacman >/dev/null; then
  manager=pacman
  packages=(base-devel pkgconf curl ca-certificates python python-pip tk libx11 alsa-lib systemd-libs
    libxcursor libxrandr libxi wayland libxkbcommon libxkbcommon-x11 openssl lzo vulkan-icd-loader)
elif command -v dnf >/dev/null; then
  manager=dnf
  packages=(gcc gcc-c++ make pkgconf-pkg-config curl ca-certificates python3 python3-pip python3-tkinter
    libX11-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel libxkbcommon-x11 openssl-devel lzo-devel vulkan-loader)
elif (( ! skip_system )); then
  printf '%s\n' 'Automatic system setup supports apt, pacman and dnf. Install the README dependencies and use --skip-system.' >&2
  exit 1
fi

missing=()
for package in "${packages[@]}"; do
  case "$manager" in
    apt) [[ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)" == 'install ok installed' ]] || missing+=("$package") ;;
    pacman) pacman -Q "$package" >/dev/null 2>&1 || missing+=("$package") ;;
    dnf) rpm -q "$package" >/dev/null 2>&1 || missing+=("$package") ;;
  esac
done

find_python() {
  local candidate
  for candidate in "${ESPER_PYTHON:-}" "$project_dir/.venv/bin/python" python3 python python3.15 python3.14 python3.13 python3.12 python3.11; do
    [[ -n "$candidate" ]] || continue
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import sys; sys.exit(not ((3, 11) <= sys.version_info < (4, 0)))' >/dev/null 2>&1; then
      "$candidate" -c 'import sys; print(sys.executable)'
      return
    fi
  done
  return 1
}
python_bin="$(find_python || true)"
toolchain="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)"
[[ -n "$toolchain" ]] || { printf '%s\n' 'Cannot read rust-toolchain.toml.' >&2; exit 1; }
printf 'Python: %s\n' "${python_bin:-will install/check system Python}"
if (( skip_system )); then
  printf '%s\n' 'System package installation: skipped.'
elif (( ${#missing[@]} )); then
  printf 'Missing system packages (%s): %s\n' "$manager" "${missing[*]}"
else
  printf '%s\n' 'System packages: already installed.'
fi
printf '%s\n' "Plan: set up local Rust $toolchain; create/reuse .venv; install Python requirements;" \
  'verify OpenSSL/LZO; build target/release/esperust.' \
  'This may download packages from your distribution, rust-lang.org and PyPI.' \
  'System packages may require sudo. Graphics drivers are left to your system.'
(( ! dry_run )) || exit 0
read -r -p 'Allow these installations and build the release? [y/N] ' answer || answer=''
case "${answer,,}" in y|yes) ;; *) printf '%s\n' 'Cancelled. Nothing was installed.'; exit 0 ;; esac

as_root() {
  if (( EUID == 0 )); then "$@"; else
    command -v sudo >/dev/null || { printf '%s\n' 'sudo is required to install system packages.' >&2; return 1; }
    sudo "$@"
  fi
}
if (( ! skip_system && ${#missing[@]} )); then
  case "$manager" in
    apt) as_root apt-get update; as_root apt-get install -y "${missing[@]}" ;;
    pacman) as_root pacman -S --needed --noconfirm "${missing[@]}" ;;
    dnf) as_root dnf install -y "${missing[@]}" ;;
  esac
fi
python_bin="$(find_python)" || { printf '%s\n' 'Python 3.11+ is still missing. Install a supported version and rerun.' >&2; exit 1; }
if [[ ! -x .venv/bin/python ]]; then
  "$python_bin" -m venv .venv
fi
.venv/bin/python -c 'import sys; assert sys.version_info >= (3,11), "Recreate .venv with Python 3.11+"'
.venv/bin/python -m ensurepip --upgrade
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install -r tools/requirements.txt
.venv/bin/python tools/setup_importer.py

export CARGO_HOME="${CARGO_HOME:-$project_dir/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$project_dir/.rustup}"
export PATH="$CARGO_HOME/bin:$PATH"
if ! command -v rustup >/dev/null; then
  command -v curl >/dev/null || { printf '%s\n' 'curl is required to install Rust.' >&2; exit 1; }
  mkdir -p .build
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o .build/rustup-init.sh
  sh .build/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain none
fi
rustup toolchain install "$toolchain" --profile minimal
rustup run "$toolchain" cargo build --release --locked
printf '\nRelease built: %s/target/release/esperust\n' "$project_dir"
printf '%s\n' 'Run it from this folder, or pass --resources /path/to/client.'
