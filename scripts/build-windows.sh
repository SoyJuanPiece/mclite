#!/usr/bin/env bash
# Build McLite for Windows (x86_64) from Linux using the nix mingw-w64 toolchain.
#
# Usage:   ./scripts/build-windows.sh
# Output:  target/x86_64-pc-windows-gnu/release/mclite.exe
#
# Requirements:
#   - nix with nixpkgs (for pkgsCross.mingwW64.stdenv.cc)
#   - rustup target x86_64-pc-windows-gnu installed:
#       rustup target add x86_64-pc-windows-gnu
#
# Gotchas this script works around (both hit and verified during development):
#
#  1) libpthread: Rust's std for windows-gnu links with `-l:libpthread.a`, but the
#     nix mingw toolchain ships mcfgthreads (libmcfgthread.a) instead of winpthread.
#     Fix: create a stub dir with libpthread.a -> libmcfgthread.a and pass
#     `-L <stubdir>` to the linker via `cargo rustc`.
#
#  2) Host CC leaking into cross builds: `nix-shell` exports CC=gcc/AR=ar for the
#     host, so the `cc` crate would compile ring/zstd-sys as ELF64 host objects and
#     the final link fails with hundreds of undefined refs (ring_core_* / ZSTD_*).
#     Fix: set CC_x86_64_pc_windows_gnu / AR_x86_64_pc_windows_gnu to the mingw
#     cross-compiler, and purge stale cached artifacts for ring/zstd-sys.

set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="x86_64-pc-windows-gnu"
OUT="target/$TARGET/release/mclite.exe"
MCFGTHREADS="/nix/store/qlw9brfc0ybafq286l86nqwcsyi062rz-mcfgthreads-x86_64-w64-mingw32-git/lib/libmcfgthread.a"
STUB_LIBS="/tmp/mclite-mingw-libs"
T="target/$TARGET/release"

# Cross CC/AR for build scripts (cc crate). Also keep host gcc on PATH for
# build scripts that must run on the host (proc-macros etc. are fine, but the
# nix-shell env may need it).
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar

# Stub libpthread.a -> libmcfgthread.a (gotcha 1)
mkdir -p "$STUB_LIBS"
ln -sf "$MCFGTHREADS" "$STUB_LIBS/libpthread.a"

# Purge stale host-compiled artifacts (gotcha 2); cheap when already clean.
rm -rf "$T/build/ring-"* "$T/build/zstd-sys-"* 2>/dev/null || true
rm -rf "$T/.fingerprint/ring-"* "$T/.fingerprint/zstd-sys-"* 2>/dev/null || true
rm -f  "$T/deps/libring-"* "$T/deps/libzstd_sys-"* 2>/dev/null || true

run_in_nix() {
  if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    # Already inside a shell with the cross toolchain.
    PATH="/nix/store/vr15iyyykg9zai6fpgvhcgyw7gckl78w-gcc-wrapper-14.3.0/bin:$PATH" \
      cargo rustc --release --target "$TARGET" --bin mclite -- -L "$STUB_LIBS"
  else
    nix-shell -p pkgsCross.mingwW64.stdenv.cc --run "
      export PATH='/nix/store/vr15iyyykg9zai6fpgvhcgyw7gckl78w-gcc-wrapper-14.3.0/bin:'\"\$PATH\"
      export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
      export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
      cargo rustc --release --target $TARGET --bin mclite -- -L $STUB_LIBS
    "
  fi
}

run_in_nix

if [[ -f "$OUT" ]]; then
  # Hash para el auto-update: <sha256>  <nombre> (formato sha256sum), junto al exe.
  sha256sum "$OUT" | sed 's#target/x86_64-pc-windows-gnu/release/##' > "$OUT.sha256"
  echo
  echo "OK: $OUT ($(numfmt --to=iec --suffix=B "$(stat -c%s "$OUT")" 2>/dev/null || stat -c%s "$OUT"))"
  echo "    hash: $(cut -d' ' -f1 "$OUT.sha256")"
else
  echo "ERROR: build finished but $OUT not found" >&2
  exit 1
fi
