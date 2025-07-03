use clap::{Parser, ValueHint};
use evm_cfg::{
    analyzer::TransactionAnalyzer,
    blockchain::EthersBlockchainService,
    config::Config,
};
use eyre::{eyre, Result};
use std::path::Path;

#[derive(Parser, Debug)]
#[command(author, version, about = "EVM交易流程可视化引擎", long_about = None)]
struct Args {
    /// 交易踪迹文件路径，包含debug_traceTransaction的输出结果（JSON格式）
    #[clap(long, value_hint = ValueHint::FilePath, value_name = "PATH_TO_TRACE_FILE")]
    pub trace: String,

    /// 输出的dot文件路径
    #[clap(long, value_hint = ValueHint::FilePath, value_name = "OUTPUT_DOT_FILE")]
    pub output: String,

    /// 是否自动转换为图片格式（需要安装Graphviz）
    #[clap(long, default_value = "false")]
    pub render: bool,

    /// 输出图片格式（仅在render=true时有效）
    #[clap(long, default_value = "svg")]
    pub format: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 解析命令行参数
    let args = Args::parse();
    
    // 检查文件路径
    if !Path::new(&args.trace).exists() {
        return Err(eyre!("交易踪迹文件不存在: {}", args.trace));
    }
    
    // 加载配置
    let config = Config::new().map_err(|e| {
        eyre!("配置加载失败: {}。请确保在项目根目录下创建.env文件并配置GETH_API", e)
    })?;
    
    println!("🔍 正在分析交易踪迹...");
    
    // 从踪迹文件创建分析器
    let mut analyzer = TransactionAnalyzer::from_trace_file(&args.trace)?;
    
    println!("📝 识别到 {} 个合约地址", analyzer.contract_addresses.len());
    
    // 创建区块链服务
    let blockchain_service = EthersBlockchainService::new(&config.rpc_url)?;
    
    // 获取所有合约字节码
    println!("⬇️ 正在从RPC节点获取合约字节码...");
    analyzer.fetch_bytecodes(&blockchain_service).await?;
    println!("✅ 成功获取 {} 个合约的字节码", analyzer.bytecode_cache.cache.len());
    
    // 生成每个合约的CFG
    println!("🔄 正在生成每个合约的控制流图...");
    analyzer.generate_contract_cfgs()?;
    
    // 构建全局交易图
    println!("🔗 正在构建全局交易执行图...");
    analyzer.build_global_transaction_graph()?;
    
    // 保存为dot文件
    println!("💾 正在保存全局交易图到 {}...", args.output);
    analyzer.save_global_graph_dot(&args.output)?;
    
    // 如果需要，转换为图片
    if args.render {
        let output_image = args.output.replace(".dot", &format!(".{}", args.format));
        println!("🎨 正在渲染图片到 {}...", output_image);
        analyzer.convert_to_image(&args.output, &output_image)?;
    }
    
    println!("✨ 分析完成！");
    
    Ok(())
}
