use async_trait::async_trait;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{H160, BlockId, BlockNumber, Bytes},
};
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait BlockchainService {
    async fn get_code(&self, address: H160) -> Result<Bytes>;
}

pub struct EthersBlockchainService {
    provider: Arc<Provider<Http>>,
}

impl EthersBlockchainService {
    pub fn new(rpc_url: &str) -> Result<Self> {
        let provider = Provider::<Http>::try_from(rpc_url)?;
        Ok(Self {
            provider: Arc::new(provider),
        })
    }
}

#[async_trait]
impl BlockchainService for EthersBlockchainService {
    async fn get_code(&self, address: H160) -> Result<Bytes> {
        let code = self
            .provider
            .get_code(address, Some(BlockId::Number(BlockNumber::Latest)))
            .await?;
        Ok(code)
    }
}

pub struct BytecodeCache {
    pub cache: HashMap<H160, Bytes>,
}

impl BytecodeCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn get(&self, address: &H160) -> Option<&Bytes> {
        self.cache.get(address)
    }

    pub fn insert(&mut self, address: H160, bytecode: Bytes) {
        self.cache.insert(address, bytecode);
    }
}

pub async fn fetch_all_bytecodes(
    addresses: &[H160],
    blockchain_service: &impl BlockchainService,
) -> Result<BytecodeCache> {
    let mut cache = BytecodeCache::new();

    for address in addresses {
        let bytecode = blockchain_service.get_code(*address).await?;
        
        // 只保存非空合约
        if !bytecode.0.is_empty() {
            cache.insert(*address, bytecode);
        }
    }

    Ok(cache)
}
