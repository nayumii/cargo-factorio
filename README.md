# cargo-factorio

Cargo subcommand to zip and install Factorio mods with the correct `<name>_<version>/info.json` layout.

## Install
```bash
cargo install cargo-factorio
```

## Usage

from a repo root that has subfolders with info.json

```bash
cargo factorio install           # installs all detected mods
cargo factorio install planets   # installs just ./planets
```

It will zip and put mods in /build and install the mods in your factorio mods folder depending on the OS. (Windows, Linux and MacOS supported)

