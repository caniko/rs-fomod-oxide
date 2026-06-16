# fomod-oxide

<!-- simit:badges:start -->
[![CI](https://img.shields.io/badge/CI-drift-2088ff)](.forgejo/workflows/ci.yaml) [![Nix](https://img.shields.io/badge/Nix-managed-5277c3)](flake.nix) [![crates.io](https://img.shields.io/badge/crates.io-ready-f46623)](https://crates.io/crates/fomod-oxide)
<!-- simit:badges:end -->

A Rust library for parsing and evaluating [FOMOD](https://fomod-docs.readthedocs.io/) mod installer configurations.

FOMOD is an XML-based format used by mod managers (Mod Organizer 2, Vortex, etc.) to define guided installation wizards for game mods.

## Features

- **Full FOMOD XML parsing** — `ModuleConfig.xml` and `info.xml` deserialization via `quick-xml` + `serde`
- **Condition evaluation** — composite AND/OR dependency trees over file states, flags, game version, and nested conditions
- **Interactive installer** — step-through API with visibility conditions, group constraint validation (`SelectExactlyOne`, `SelectAtMostOne`, `SelectAtLeastOne`, `SelectAll`, `SelectAny`), and automatic flag propagation on selection
- **Install plan resolution & execution** — collects file operations from required files, selected plugins, and conditional file installs, sorted by priority; execute the plan to copy files into a target directory
- **Declarative configuration** — skip the interactive wizard entirely by specifying all selections upfront as a name-based config; includes SRI hash integrity verification (nixpkgs `rev`/`hash` pattern), schema versioning, and automatic default/all-options template generation
- **Nix output** (`nix` feature) — serialize declarative configs to Nix expressions or NixOS module fragments via `ronix`
- **CLI** (`cli` feature) — generate declarative configs from FOMOD installers in Nix, RON, or JSON format

## Usage

### Library

```rust
use fomod_oxide::{ModuleConfig, Installer};

// Parse the installer configuration
let xml = std::fs::read_to_string("fomod/ModuleConfig.xml")?;
let config = ModuleConfig::parse(&xml)?;

// Create an installer and walk through steps
let mut installer = Installer::new(config);

for (idx, step) in installer.visible_steps() {
    println!("Step {idx}: {}", step.name);
    // Present groups/plugins to the user, collect selections...
}

// Record selections and resolve the install plan
installer.select(0, 0, vec![0]);
let plan = installer.resolve();
for op in &plan.operations {
    println!("{} -> {}", op.source, op.destination);
}
```

### Declarative mode

Generate a config template, then load and apply it to install without interaction:

```rust
use std::path::Path;
use fomod_oxide::{ModuleConfig, DeclarativeConfig, Installer};

let xml = std::fs::read_to_string("fomod/ModuleConfig.xml")?;
let config = ModuleConfig::parse(&xml)?;

// Generate a template with default selections and save it
let decl = DeclarativeConfig::from_defaults(&xml, "1.0.0", &config);
std::fs::write("fomod-config.json", decl.to_json()?)?;

// Later: load the config and install non-interactively
let decl = DeclarativeConfig::from_json(&std::fs::read_to_string("fomod-config.json")?)?;
let mut installer = Installer::new(config);
decl.apply(&xml, &mut installer)?;
let plan = installer.resolve();
plan.execute(Path::new("unpacked_mod/"), Path::new("game/Data/"))?;
```

### CLI

```sh
# Generate a Nix declarative config (default selections)
fomod-oxide ModuleConfig.xml --info info.xml

# Generate with all options listed, output as JSON
fomod-oxide ModuleConfig.xml --all --format json

# Specify rev and output file
fomod-oxide ModuleConfig.xml --rev "2.1.0" -o config.nix
```

## Cargo features

| Feature | Description |
|---------|-------------|
| `json` | JSON serialization/deserialization for declarative configs |
| `ron` | RON serialization/deserialization for declarative configs |
| `nix` | Nix expression output via `ronix` |
| `cli` | Builds the `fomod-oxide` binary (enables `json`, `ron`, `nix`, `clap`) |

## CI

Woodpecker CI on Codeberg runs `nix flake check` (build, tests, clippy, fmt) on every push and pull request.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
