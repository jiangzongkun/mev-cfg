use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct TraceStep {
    pub pc: Option<u16>,
    pub address: Option<HashMap<String, u8>>,
    // 你可以根据需要添加其它字段
    // pub op: Option<String>,
    // pub stack: Option<Vec<String>>,
}

impl TraceStep {
    /// 将 address 字段（HashMap）转为 0x 开头的 hex 字符串
    pub fn address_hex(&self) -> Option<String> {
        self.address.as_ref().map(|map| {
            let mut bytes: Vec<u8> = vec![];
            for i in 0..map.len() {
                if let Some(b) = map.get(&i.to_string()) {
                    bytes.push(*b);
                }
            }
            format!("0x{}", hex::encode(bytes))
        })
    }
}

pub fn parse_trace_file(path: &str) -> eyre::Result<Vec<TraceStep>> {
    let data = std::fs::read_to_string(path)?;
    let steps: Vec<TraceStep> = serde_json::from_str(&data)?;
    Ok(steps)
}