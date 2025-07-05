use dotenv::dotenv;
use eyre::{eyre, Result};
use std::env;

pub struct Config {
    pub rpc_url: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        // Load .env file
        dotenv().ok();

        // Read RPC URL
        let rpc_url = env::var("GETH_API")
            .map_err(|_| eyre!("GETH_API environment variable not found. Please configure GETH_API=<Your RPC Node URL> in the .env file"))?;

        Ok(Config { rpc_url })
    }
}
