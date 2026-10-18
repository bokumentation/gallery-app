#!/usr/bin/env bash
set -euo pipefail

# Installs the nvJPEG runtime locally without sudo. The extracted library is
# picked up automatically by the app from ~/.local/lib/rust-gallery.
#
# Requires the NVIDIA CUDA apt repository (the same one that provides the
# driver packages). If apt cannot find the packages, install them system-wide
# with: sudo apt install libnvjpeg-13-1 libnvjpeg-dev-13-1

dest="${HOME}/.local/lib/rust-gallery"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

(
    cd "${tmp}"
    apt-get download libnvjpeg-13-1 libnvjpeg-dev-13-1
)

mkdir -p "${dest}/nvjpeg"
for deb in "${tmp}"/*.deb; do
    dpkg-deb -x "${deb}" "${dest}/nvjpeg"
done

library="$(find "${dest}/nvjpeg" -type f -name 'libnvjpeg.so.13*' | head -n 1)"
if [[ -z "${library}" ]]; then
    echo "libnvjpeg.so.13 was not found in the downloaded packages" >&2
    exit 1
fi

ln -sfn "${library}" "${dest}/libnvjpeg.so.13"
echo "nvJPEG installed at ${dest}/libnvjpeg.so.13"
