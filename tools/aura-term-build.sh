#!/bin/sh
# Build aura-term the way Arch builds its foot package, so budget
# measurements compare like with like. Runs on Arch (the VM), not the host.
#
#   aura-term-build.sh pgo <srcdir> <builddir>   # release: PGO + LTO (Arch recipe)
#   aura-term-build.sh dev <srcdir> <builddir>   # quick: no PGO
#
# Arch's foot PKGBUILD (gitlab.archlinux.org/archlinux/packaging/packages/foot):
#   ./pgo/pgo.sh full-headless-sway . build -Dterminfo-base-name=foot-extra
#       --prefix=/usr --wrap-mode=nodownload
# with makepkg.conf's CFLAGS/LDFLAGS, gcc. pgo.sh adds -O3 and -Db_lto=true,
# profiles by running aura-termd in a private headless sway, rebuilds with
# -Db_pgo=use. Deviations here: prefix /usr/local, no docs/themes (no effect
# on the binaries), terminfo name stays foot (installed under
# share/aura-term/terminfo), and at most 2 build jobs (shared 2.5 GB VM).
set -eu

mode=${1:?usage: $0 pgo|dev <srcdir> <builddir>}
src=$(realpath "${2:?srcdir}")
bld=${3:?builddir}

# Same compiler flags as makepkg
# shellcheck disable=SC1091
. /etc/makepkg.conf
export CFLAGS LDFLAGS

# Cap ninja at -j2 (pgo.sh calls plain 'ninja')
shim=$(mktemp -d)
trap 'rm -rf "$shim"' EXIT INT TERM
printf '#!/bin/sh\nexec /usr/bin/ninja -j2 "$@"\n' > "$shim/ninja"
chmod +x "$shim/ninja"
PATH=$shim:$PATH

opts="--prefix=/usr/local --wrap-mode=nodownload -Ddocs=disabled -Dthemes=false -Db_lto_threads=2 -Dsystemd-units-dir=/usr/local/lib/systemd/user"

case $mode in
pgo)
    # pgo.sh reuses an existing dir's options; start clean for a true Arch-like build
    rm -rf "$bld"
    # shellcheck disable=SC2086
    "$src/pgo/pgo.sh" full-headless-sway "$src" "$bld" $opts
    ;;
dev)
    [ -d "$bld" ] || meson setup --buildtype=release -Dtests=false $opts "$bld" "$src"
    ninja -C "$bld"
    ;;
*)
    echo "unknown mode: $mode" >&2; exit 1 ;;
esac

echo "built: $("$bld"/aura-termd --version)"
