# firstaide

[![Build Status](https://travis-ci.com/allenap/firstaide.svg?branch=master)](https://travis-ci.com/allenap/firstaide)

This is a bootstrapping tool that'll build, cache, and clean your environment.
It's intended for use with [direnv][] and [Nix][].


## How to use:

Install `firstaide`. For example, if you have [installed Rust][install-rust],
and cloned this repo somewhere, you can:

```shell
cargo install --path /path/to/firstaide/repo
```

In your project, add a `.firstaide.toml` configuration file with at least the
following settings:

```toml
cache_dir = "path/to/dir"
build_exe = "path/to/exe"
watch_exe = "path/to/exe"
```

`cache_dir` is a directory, relative to `.firstaide.toml`, where firstaide will
store its cache and put other files it needs a place for. Calling `firstaide
clean` will remove this directory, so choose wisely. It's a good idea to add
this to `.gitignore` too.

`build_exe` is an executable or script that will build your environment. It
**must** accept as arguments a command to be run within that environment. For
example, `build_exe` might point to a script like this:

```bash
#!/usr/bin/env bash
exec nix-shell --run "$(printf '%q ' "$@")"
```

`watch_exe` is an executable or script that emits a null-separated list of
filenames for direnv to watch; firstaide passes these names to direnv's
`watch_file` function. For example, the following script would ask direnv to
watch all the files in `etc` and `nix` recursively:

```bash
#!/usr/bin/env bash
exec git ls-files -z -- etc nix
```

Add the following to `.envrc`:

```bash
eval "$(firstaide hook)"
```

Then run `firstaide build` (or `firstaide --help`).

### Messages

the `messages` section of `.firstaide.toml` allows you to configure the help text shown by direnv when the environment is inactive, stale, and loaded

by default the messages are set as follows:

```toml
[messages]
getting_started = "firstaide --help" # displayed when direnv loads successfully
stale = "firstaide build"
inactive = "build firstaide and run \"firstaide build\""
```

## To develop:

First, [install the Rust development tools][install-rust]. Then:

```shell
cargo build  # Compiles a debug executable.
cargo test   # Compiles and tests.
cargo run    # Compiles and runs a debug executable.
# ...
```


[install-rust]: https://www.rust-lang.org/tools/install
[direnv]: https://direnv.net/
[nix]: https://nixos.org/nix/


## Making a release

1. Bump version in [`Cargo.toml`](Cargo.toml) and [`default.nix`](default.nix).
2. Build **and** test: `cargo build && cargo test`. The latter on its own does
   do a build, but a test build can hide warnings about dead code, so do both.
3. Run `nix-build` to ensure that the package builds. The `cargoSha256` value
   may need to be updated, for example.
4. Commit with message "Bump version to `$VERSION`."
5. Tag with "v`$VERSION`", e.g. `git tag v1.0.10`.
6. Push: `git push && git push --tags`.
