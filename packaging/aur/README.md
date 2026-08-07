# AUR packages

Two packages, both tracking the same upstream version:

- `solitaire-tui` builds from the GitHub release tarball with `cargo`, and runs
  the test suite in `check()` before packaging.
- `solitaire-tui-bin` unpacks the statically linked musl binary from the same
  release, so it installs in a second and depends on nothing.

Each AUR package is its own git repository containing exactly two files at the
root, `PKGBUILD` and `.SRCINFO`. The copies here are the source of truth; the AUR
repositories are a place to push them.

## Publishing a package for the first time

Add an SSH key at <https://aur.archlinux.org/account> first, then:

```sh
git clone ssh://aur@aur.archlinux.org/solitaire-tui.git aur-solitaire-tui
cp packaging/aur/solitaire-tui/{PKGBUILD,.SRCINFO} aur-solitaire-tui/
cd aur-solitaire-tui
git add PKGBUILD .SRCINFO
git commit -m 'Initial import: solitaire-tui 0.1.0'
git push
```

Cloning a name nobody has taken yet is not an error — the AUR creates the
repository on first push.

## Releasing a new version

Both `PKGBUILD`s pin `pkgver` and the source checksums, so each release needs
both updated. From a clean checkout of the new tag:

```sh
# source package — checksum of the GitHub tag tarball
makepkg -g -p packaging/aur/solitaire-tui/PKGBUILD

# binary package — checksums of the two musl release assets
makepkg -g -p packaging/aur/solitaire-tui-bin/PKGBUILD
```

Bump `pkgver`, paste the sums in, reset `pkgrel` to 1, then regenerate the
metadata and verify the package actually builds:

```sh
cd packaging/aur/solitaire-tui
makepkg --printsrcinfo > .SRCINFO
makepkg -f            # builds and runs the tests
```

`.SRCINFO` must be regenerated whenever `PKGBUILD` changes — the AUR reads
package metadata from it, not from the `PKGBUILD`, and a stale one shows the
wrong version on the package page.

Bump `pkgrel` instead of `pkgver` when only the packaging changed and upstream
did not.

## Note on the binary name

Both packages install `/usr/bin/solitaire-tui`. The binary was called
`solitaire` up to 0.1.0, which collided with the AUR's `solitaire` (GNOME's
patience game); the rename in 0.2.0 is what removed the need for a
`conflicts=('solitaire')` on both packages.
