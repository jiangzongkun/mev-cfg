use clap::{Parser, ValueHint, ArgGroup};
use evm_cfg::{
    analyzer::TransactionAnalyzer,
    blockchain::{EthersBlockchainService, save_transaction_trace},
    config::Config,
};
use eyre::{eyre, Result};
use std::path::Path;
use ethers::types::H256;

#[derive(Parser, Debug)]
#[command(author, version, about = "EVM Transaction Flow Visualization Engine", long_about = None)]
#[clap(group(ArgGroup::new("input").required(true).args(&["trace", "tx_hash"])))]
struct Args {
    /// Path to transaction trace file containing debug_traceTransaction output (JSON format)
    #[clap(long, value_hint = ValueHint::FilePath, value_name = "PATH_TO_TRACE_FILE")]
    pub trace: Option<String>,

    /// Transaction hash (automatically fetch trace)
    #[clap(long, value_name = "TRANSACTION_HASH")]
    pub tx_hash: Option<String>,

    /// Output DOT file path
    #[clap(long, value_hint = ValueHint::FilePath, value_name = "OUTPUT_DOT_FILE")]
    pub output: Option<String>,

    /// Automatically convert to image format (requires Graphviz)
    #[clap(long, default_value = "false")]
    pub render: bool,

    /// Output image format (only valid when render=true)
    #[clap(long, default_value = "svg")]
    pub format: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Load configuration
    let config = Config::new().map_err(|e| {
        eyre!("Configuration loading failed: {}. Please ensure you have created a .env file in the project root and configured GETH_API", e)
    })?;
    
    // Create blockchain service
    let blockchain_service = EthersBlockchainService::new(&config.rpc_url)?;
    
    // Determine transaction trace path (from file or via transaction hash)
    let trace_path = if let Some(trace_file) = &args.trace {
        // Use user-provided trace file
        if !Path::new(trace_file).exists() {
            return Err(eyre!("Transaction trace file does not exist: {}", trace_file));
        }
        trace_file.clone()
    } else if let Some(tx_hash_str) = &args.tx_hash {
        // Get trace from transaction hash
        // Parse transaction hash
        let tx_hash = tx_hash_str.parse::<H256>()
            .map_err(|_| eyre!("Invalid transaction hash: {}", tx_hash_str))?;
        
        println!("🔍 Fetching trace for transaction {} from blockchain...", tx_hash);
        
        // Get and save trace
        let trace_file = save_transaction_trace(tx_hash, &blockchain_service).await?;
        println!("✅ Transaction trace saved to {}", trace_file);
        
        trace_file
    } else {
        return Err(eyre!("You must provide either a transaction trace file (--trace) or a transaction hash (--tx_hash)"));
    };
    
    // Determine output file path
    let output_path = if let Some(output_file) = &args.output {
        output_file.clone()
    } else if let Some(tx_hash) = &args.tx_hash {
        // Name output file after transaction hash and save in Results directory
        let output_dir = "Results";
        if !Path::new(output_dir).exists() {
            std::fs::create_dir_all(output_dir)?;
        }
        format!("{}/{}.dot", output_dir, tx_hash)
    } else {
        // Generate output path from trace file path
        let trace_filename = Path::new(&trace_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        
        let output_dir = "Results";
        if !Path::new(output_dir).exists() {
            std::fs::create_dir_all(output_dir)?;
        }
        
        format!("{}/{}.dot", output_dir, trace_filename.replace(".txt", ""))
    };
    
    println!("🔍 Analyzing transaction trace...");
    
    // Create analyzer from trace file
    let mut analyzer = TransactionAnalyzer::from_trace_file(&trace_path)?;
    
    println!("📝 Identified {} contract addresses", analyzer.contract_addresses.len());
    
    // Get all contract bytecodes
    println!("⬇️ Fetching contract bytecodes from RPC node...");
    analyzer.fetch_bytecodes(&blockchain_service).await?;
    println!("✅ Successfully fetched bytecodes for {} contracts", analyzer.bytecode_cache.cache.len());
    
    // Generate CFG for each contract
    println!("🔄 Generating control flow graphs for each contract...");
    analyzer.generate_contract_cfgs()?;
    
    // Build global transaction graph
    println!("🔗 Building global transaction execution graph...");
    analyzer.build_global_transaction_graph()?;
    
    // Save to dot file
    println!("💾 Saving global transaction graph to {}...", output_path);
    analyzer.save_global_graph_dot(&output_path)?;
    
    // Convert to image if requested
    if args.render {
        let output_image = output_path.replace(".dot", &format!(".{}", args.format));
        println!("🎨 Rendering image to {}...", output_image);
        analyzer.convert_to_image(&output_path, &output_image)?;
    }
    
    println!("✨ Analysis complete!");
    
    Ok(())
}
