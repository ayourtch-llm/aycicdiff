use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

use aycicdiff::rules::RulesConfig;

/// Cisco IOS/IOS-XE config diff utility.
///
/// Generates incremental configuration (suitable for `copy file run`)
/// to transform a running config into a target config.
#[derive(Parser, Debug)]
#[command(name = "aycicdiff", version, about)]
struct Cli {
    /// Path to the running config file (or "-" for stdin)
    #[arg(short = 'r', long = "running")]
    running: String,

    /// Path to the target config file
    #[arg(short = 't', long = "target")]
    target: PathBuf,

    /// Path to "show version" output file (optional, for version-aware behavior)
    #[arg(short = 'v', long = "version-file")]
    version_file: Option<PathBuf>,

    /// Path to rules config file (TOML, extends built-in rules)
    #[arg(short = 'c', long = "rules")]
    rules_file: Option<PathBuf>,

    /// Write output to file instead of stdout
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Show what would be generated without writing (implies verbose)
    #[arg(long)]
    dry_run: bool,

    /// Enable verbose output
    #[arg(long)]
    verbose: bool,

    /// Dump the effective rules (built-in + user) and exit
    #[arg(long)]
    dump_rules: bool,
}

fn read_input(path: &str) -> Result<String, io::Error> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        fs::read_to_string(path)
    }
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    // Load rules
    let rules = match &cli.rules_file {
        Some(path) => match RulesConfig::load_from_file(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error loading rules: {}", e);
                process::exit(1);
            }
        },
        None => RulesConfig::builtin(),
    };

    if cli.dump_rules {
        // Serialize effective rules as TOML for inspection
        println!("{}", rules.to_toml());
        return;
    }

    let running = match read_input(&cli.running) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading running config '{}': {}", cli.running, e);
            process::exit(1);
        }
    };

    let target = match fs::read_to_string(&cli.target) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading target config '{}': {}", cli.target.display(), e);
            process::exit(1);
        }
    };

    let show_version = cli.version_file.as_ref().map(|path| {
        fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Error reading version file '{}': {}", path.display(), e);
            process::exit(1);
        })
    });

    let delta = aycicdiff::generate_delta_with_rules(
        &running,
        &target,
        show_version.as_deref(),
        &rules,
    );

    if cli.verbose || cli.dry_run {
        eprintln!("--- Generated delta ({} bytes) ---", delta.len());
    }

    if delta.is_empty() {
        if cli.verbose || cli.dry_run {
            eprintln!("No changes needed.");
        }
        return;
    }

    if cli.dry_run {
        println!("{}", delta);
        return;
    }

    match cli.output {
        Some(ref path) => {
            if let Err(e) = fs::write(path, &delta) {
                eprintln!("Error writing output to '{}': {}", path.display(), e);
                process::exit(1);
            }
            if cli.verbose {
                eprintln!("Delta written to {}", path.display());
            }
        }
        None => {
            print!("{}", delta);
        }
    }
}
