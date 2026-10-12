# Studio

An Affinity-style all-in-one photo app in Rust: photo library, raw development and pixel
editing on one shared `.studio` document, with a persona bar to switch modes in one click.
Each persona is the open-source [storytold](https://github.com/storytold) app it embeds,
looking and working exactly as that app does:

| Persona | Is |
| --- | --- |
| Library, Develop | [LightCraft](https://github.com/storytold/lightcraft) |
| Pixel | [PhotoCraft](https://github.com/storytold/photocraft) |
| Film | Studio's own (planned): halation, grain, blur, bleach |

> **Pre-release.** Studio is under active development and not ready for real work: file
> formats and commands still change. Progress is tracked in [`PROGRESS.md`](PROGRESS.md),
> the plan in [`PRD.md`](PRD.md).

## Build

Needs stable Rust. Windows and Linux, on x64 and ARM64.

- **Windows:** the MSVC toolchain with Visual Studio Build Tools 2022 (C++ tools for your
  architecture). Run cargo from PowerShell or cmd, not Git Bash (its `link` shadows MSVC's).
- **Linux:** `libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev`.

```sh
cargo run --release -p studio
```

## Tests

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

On a machine short of memory, test one crate at a time (`-p studio-doc`, `-p studio-shell`,
`-p studio`) with `-- --test-threads=2`. The GPU render and smoke tests are opt-in:
`cargo test -p studio-shell -- --ignored render_personas` and
`cargo test -p studio --test smoke -- --ignored` (opens a real window).

## License

GPL-3.0-or-later; see [`LICENSE`](LICENSE). LightCraft and PhotoCraft (MIT OR Apache-2.0) are
by the storytold contributors; they and the fonts, icons and crates they bring are credited in
[`NOTICE`](NOTICE).

Studio is an independent project. It is not affiliated with, endorsed by or sponsored by
Adobe, Affinity (Canva) or ArtCraft. Product names are trademarks of their owners and are used
only to describe what Studio resembles.
