use clap::{Parser, Subcommand};
use continue_cart::{
    generate_market_index, inspect_path, pack_source_tree, resolve_resources, unpack_bundle,
    verify_bundle, verify_system_catalog_bundle, CartMarketIndex, CartMarketSource,
    ResourceResolveRequest, ResourceTrustClass, ResourceType,
};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Parser)]
#[command(name = "continue-cart")]
#[command(about = "Continue Cart Toolchain MVP")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Inspect {
        path: PathBuf,
    },
    Pack {
        cart_dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Verify {
        cart: PathBuf,
    },
    VerifySystemCatalog {
        cart: PathBuf,
        #[arg(long)]
        sha256: String,
    },
    Unpack {
        cart: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Index {
        inputs: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "local.generated")]
        source_id: String,
        #[arg(long, default_value = "Local Generated")]
        source_name: String,
    },
    Resolve {
        indexes: Vec<PathBuf>,
        #[arg(long = "type")]
        resource_type: String,
        #[arg(long)]
        id: String,
        #[arg(long, default_value = ">=0.0.0")]
        version: String,
        #[arg(long, default_value = "other")]
        requester_trust: String,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Inspect { path } => {
            print_json(&inspect_path(&path)?)?;
        }
        Command::Pack { cart_dir, out } => {
            print_json(&pack_source_tree(&cart_dir, &out)?)?;
        }
        Command::Verify { cart } => {
            print_json(&verify_bundle(&cart)?)?;
        }
        Command::VerifySystemCatalog { cart, sha256 } => {
            print_json(&verify_system_catalog_bundle(&cart, &sha256)?)?;
        }
        Command::Unpack { cart, out } => {
            print_json(&unpack_bundle(&cart, &out)?)?;
        }
        Command::Index {
            inputs,
            out,
            source_id,
            source_name,
        } => {
            let index = generate_market_index(
                &inputs,
                CartMarketSource {
                    id: source_id,
                    name: source_name,
                    trust_class: ResourceTrustClass::Other,
                },
            )?;
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out, serde_json::to_string_pretty(&index)?)?;
            print_json(&serde_json::json!({ "output": out }))?;
        }
        Command::Resolve {
            indexes,
            resource_type,
            id,
            version,
            requester_trust,
        } => {
            let indexes = read_indexes(&indexes)?;
            let report = resolve_resources(
                &indexes,
                &[ResourceResolveRequest {
                    resource_type: ResourceType::from_str(&resource_type)?,
                    resource_id: id,
                    version,
                    requester_trust_class: ResourceTrustClass::from_str(&requester_trust)?,
                }],
            )?;
            print_json(&report)?;
        }
    }
    Ok(())
}

fn read_indexes(paths: &[PathBuf]) -> Result<Vec<CartMarketIndex>, Box<dyn std::error::Error>> {
    let mut indexes = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(path)?;
        indexes.push(serde_json::from_str(&text)?);
    }
    Ok(indexes)
}

fn print_json(value: &impl serde::Serialize) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
