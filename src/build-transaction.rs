use anyhow::{anyhow, Result};
use clap::Parser;

use enokiweave::transaction_builder;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    amount: u64,

    #[arg(long)]
    /// Type of input: "public" or "confidential"
    input_type: String,

    #[arg(long)]
    /// Type of output: "public" or "confidential"
    output_type: String,

    #[arg(long)]
    /// Hex spend key mapping to hex public key of the UTXO, format is "spend_key:public_key" separated by a semicolon
    /// Required for confidential inputs
    input_keys_by_id: Option<String>,

    /// Hex recipient public key
    /// Required for confidential outputs
    #[arg(long)]
    recipient_pubkey: Option<String>,

    /// Address for public inputs
    /// Required for public inputs
    #[arg(long)]
    input_address: Option<String>,

    /// Address for public outputs
    /// Required for public outputs
    #[arg(long)]
    output_address: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate input/output types
    if !["public", "confidential"].contains(&args.input_type.as_str()) {
        return Err(anyhow!("Input type must be either 'public' or 'confidential'"));
    }
    if !["public", "confidential"].contains(&args.output_type.as_str()) {
        return Err(anyhow!("Output type must be either 'public' or 'confidential'"));
    }

    let transaction = transaction_builder::build_transaction(
        args.amount,
        &args.input_type,
        &args.output_type,
        args.input_keys_by_id.as_deref(),
        args.recipient_pubkey.as_deref(),
        args.input_address.as_deref(),
        args.output_address.as_deref(),
    )?;

    println!("{}", serde_json::to_string_pretty(&transaction)?);

    Ok(())
}
