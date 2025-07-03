use ethers::types::H160;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

#[derive(Debug, Deserialize, Clone)]
pub struct TraceStep {
    pub pc: Option<u16>,
    pub op: Option<String>,
    pub gas: Option<u64>,
    pub gasCost: Option<u64>,
    pub depth: Option<u64>,
    pub error: Option<String>,
    pub stack: Option<Vec<String>>,
    pub memory: Option<Vec<String>>,
    pub storage: Option<HashMap<String, String>>,
    pub address: Option<HashMap<String, u8>>,
}

#[derive(Debug, Deserialize)]
pub struct TraceTransaction {
    pub gas: u64,
    pub failed: bool,
    pub returnValue: String,
    pub structLogs: Vec<TraceStep>,
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

    /// 获取H160格式的地址
    pub fn get_h160_address(&self) -> Option<H160> {
        self.address_hex().and_then(|addr_hex| {
            H160::from_str(&addr_hex).ok()
        })
    }
    
    /// 判断该步骤是否为合约调用
    pub fn is_contract_call(&self) -> bool {
        match &self.op {
            Some(op) => {
                op == "CALL" || op == "DELEGATECALL" || op == "STATICCALL" || op == "CALLCODE"
            },
            None => false,
        }
    }
    
    /// 获取合约调用的目标地址（从堆栈中获取）
    pub fn get_call_target(&self) -> Option<H160> {
        if !self.is_contract_call() {
            return None;
        }
        
        // 不同的调用指令，目标地址在堆栈中的位置不同
        // CALL: [gas, address, value, argsOffset, argsLength, retOffset, retLength]
        // DELEGATECALL/STATICCALL: [gas, address, argsOffset, argsLength, retOffset, retLength]
        let stack_pos = match self.op.as_deref() {
            Some("CALL") => 1, // 地址在第2个位置 (索引1)
            Some("DELEGATECALL") | Some("STATICCALL") | Some("CALLCODE") => 1, // 地址在第2个位置
            _ => return None,
        };
        
        self.stack.as_ref().and_then(|stack| {
            if stack.len() > stack_pos {
                let addr_hex = &stack[stack.len() - 1 - stack_pos]; // 堆栈是从右向左读取的
                if addr_hex.starts_with("0x") {
                    H160::from_str(addr_hex).ok()
                } else {
                    H160::from_str(&format!("0x{}", addr_hex)).ok()
                }
            } else {
                None
            }
        })
    }
    
    /// 获取调用类型
    pub fn get_call_type(&self) -> Option<String> {
        self.op.clone()
    }
}

/// 解析交易踪迹文件
pub fn parse_trace_file(path: &str) -> eyre::Result<Vec<TraceStep>> {
    let data = std::fs::read_to_string(path)?;
    
    // 尝试直接解析为Step数组
    let steps_result: Result<Vec<TraceStep>, _> = serde_json::from_str(&data);
    
    match steps_result {
        Ok(steps) => Ok(steps),
        Err(_) => {
            // 尝试解析为TraceTransaction格式
            let trace: TraceTransaction = serde_json::from_str(&data)?;
            Ok(trace.structLogs)
        }
    }
}

/// 从踪迹中提取所有涉及的合约地址
pub fn extract_contract_addresses(steps: &[TraceStep]) -> HashSet<H160> {
    let mut addresses = HashSet::new();
    
    for step in steps {
        if let Some(addr) = step.get_h160_address() {
            addresses.insert(addr);
        }
    }
    
    addresses
}

/// 从踪迹中提取调用关系
pub struct CallEdge {
    pub from_addr: H160,
    pub from_pc: u16,
    pub to_addr: H160,
    pub call_type: String,
}

pub fn extract_call_edges(steps: &[TraceStep]) -> Vec<CallEdge> {
    let mut edges = Vec::new();
    let mut i = 0;
    
    while i < steps.len() - 1 {
        let current_step = &steps[i];
        let next_step = &steps[i + 1];
        
        if current_step.is_contract_call() {
            if let (Some(from_addr), Some(from_pc), Some(call_type)) = (
                current_step.get_h160_address(),
                current_step.pc,
                current_step.get_call_type()
            ) {
                if let Some(to_addr) = next_step.get_h160_address() {
                    edges.push(CallEdge {
                        from_addr,
                        from_pc,
                        to_addr,
                        call_type,
                    });
                }
            }
        }
        
        i += 1;
    }
    
    edges
}

/// 按地址过滤踪迹步骤
pub fn filter_steps_by_address(steps: &[TraceStep], address: &H160) -> Vec<TraceStep> {
    steps
        .iter()
        .filter(|step| {
            if let Some(addr) = step.get_h160_address() {
                &addr == address
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

/// 获取合约执行过的PC值集合
pub fn get_executed_pcs(steps: &[TraceStep]) -> HashSet<u16> {
    steps
        .iter()
        .filter_map(|step| step.pc)
        .collect()
}