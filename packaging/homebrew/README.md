# Homebrew tap

`Formula/solitaire-tui.rb` installs the prebuilt binary from the GitHub release —
macOS and Linux, Apple Silicon and x86_64. No Rust toolchain is involved, so it
installs in a second.

Homebrew taps are ordinary GitHub repositories whose name begins with
`homebrew-`. `Forsbakk/homebrew-tap` becomes `brew tap Forsbakk/tap`; the prefix
is implied and must not be typed. The formula lives at `Formula/<name>.rb`, and
the class name is the file name in CamelCase — `solitaire-tui.rb` holds
`class SolitaireTui`. They have to agree or Homebrew will not load the file.

## Publishing the tap

Create an empty `homebrew-tap` repository on GitHub, then:

```sh
git clone https://github.com/Forsbakk/homebrew-tap
mkdir -p homebrew-tap/Formula
cp packaging/homebrew/Formula/solitaire-tui.rb homebrew-tap/Formula/
cd homebrew-tap
git add Formula/solitaire-tui.rb
git commit -m 'solitaire-tui 0.2.0'
git push
```

Users then install with:

```sh
brew install Forsbakk/tap/solitaire-tui
```

## Releasing a new version

Nothing to do: the `tap` job in `.github/workflows/release.yml` handles it.

The formula pins `version` and four checksums of archives that do not exist until
the release is built, so it can never be correct *before* a tag — which is why it
is generated afterwards rather than bumped by hand. On a tag push the job runs
`update-formula.sh`, commits the result back to `main`, and pushes a copy to the
tap. The copy in this directory is therefore always one that CI wrote.

`update-formula.sh` can be run by hand too, which is the way to repair a release
whose tap push failed:

```sh
./packaging/homebrew/update-formula.sh 0.2.0            # reads the GitHub release
./packaging/homebrew/update-formula.sh 0.2.0 /tmp/sums  # or a local sums directory
```

It refuses to write anything unless all four checksums are present and look like
SHA-256 — the formula is built in a temporary file and moved into place, so a
failure leaves the previous one intact.

The tap push is skipped, without failing the release, when `HOMEBREW_TAP_TOKEN`
is unset. Structural changes to the formula — a new platform, a changed
`install` block — go in the heredoc inside `update-formula.sh`, not in the
generated `.rb`.

Verifying a formula needs a machine with Homebrew on it:

```sh
brew audit --strict --online Forsbakk/tap/solitaire-tui
brew install --verbose Forsbakk/tap/solitaire-tui
brew test Forsbakk/tap/solitaire-tui
```

Running them by hand is rarely necessary, though: the `formula-check` job does
exactly this on every tag. macOS runners come with Homebrew and are Apple
Silicon, so the formula is installed and run there — the one platform that
cannot be checked from a Linux machine. It gates the tap push, so a formula that
fails to install never reaches anyone. `brew audit` is advisory within it, since
audit's house style targets homebrew-core and a personal tap should not fail a
release over a naming preference.

## One-time setup

The tap push needs a token with write access to `Forsbakk/homebrew-tap`, because
the workflow's own `GITHUB_TOKEN` is scoped to this repository alone and cannot
push to another one:

```sh
gh repo create Forsbakk/homebrew-tap --public \
  --description 'Homebrew formulae for Forsbakk projects'

# then, with a fine-grained PAT scoped to homebrew-tap with Contents: read/write
gh secret set HOMEBREW_TAP_TOKEN -R Forsbakk/solitaire-tui
```
