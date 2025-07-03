use dotenv::dotenv;
use eyre::{eyre, Result};
use std::env;

pub struct Config {
    pub rpc_url: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        // 加载 .env 文件
        dotenv().ok();

        // 读取 RPC URL
        let rpc_url = env::var("GETH_API")
            .map_err(|_| eyre!("未找到 GETH_API 环境变量。请在 .env 文件中配置 GETH_API=<您的RPC节点URL>"))?;

        Ok(Config { rpc_url })
    }
}
