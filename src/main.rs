use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use fomod_oxide::{DeclarativeConfig, FomodInfo, ModuleConfig};

#[derive(Parser)]
#[command(name = "fomod-oxide", about = "Generate declarative configs from FOMOD installers")]
struct Cli {
    /// Path to ModuleConfig.xml
    input: PathBuf,

    /// Path to info.xml (for rev/version metadata)
    #[arg(long)]
    info: Option<PathBuf>,

    /// Override the rev field (default: version from info.xml or module name)
    #[arg(long)]
    rev: Option<String>,

    /// Output file (defaults to stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Output format
    #[arg(short, long, default_value = "nix")]
    format: Format,

    /// Include all available plugins instead of just defaults
    #[arg(long)]
    all: bool,
}

#[derive(Clone, ValueEnum)]
enum Format {
    Nix,
    Ron,
    Json,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let xml = fs::read_to_string(&cli.input)?;
    let config = ModuleConfig::parse(&xml)?;

    // Resolve rev: explicit flag > info.xml version > module name
    let rev = if let Some(rev) = cli.rev {
        rev
    } else if let Some(ref info_path) = cli.info {
        let info_xml = fs::read_to_string(info_path)?;
        let info = FomodInfo::parse(&info_xml)?;
        info.version
            .or(info.name)
            .unwrap_or_else(|| config.module_name.value.clone())
    } else {
        config.module_name.value.clone()
    };

    let decl = if cli.all {
        DeclarativeConfig::from_all(&xml, rev, &config)
    } else {
        DeclarativeConfig::from_defaults(&xml, rev, &config)
    };

    let output = match cli.format {
        Format::Nix => ronix::to_nix(&decl)?,
        Format::Ron => ron::ser::to_string_pretty(&decl, ron::ser::PrettyConfig::default())?,
        Format::Json => serde_json::to_string_pretty(&decl)?,
    };

    let output = if output.ends_with('\n') {
        output
    } else {
        format!("{output}\n")
    };

    match cli.output {
        Some(path) => fs::write(path, &output)?,
        None => io::stdout().write_all(output.as_bytes())?,
    }

    Ok(())
}
