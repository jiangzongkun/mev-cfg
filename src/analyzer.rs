use crate::blockchain::{BlockchainService, BytecodeCache};
use crate::cfg_gen::{
    cfg_graph::CFGRunner,
    dasm::{self, InstructionBlock},
    trace::{self, CallEdge, TraceStep},
};
use eyre::{eyre, Result};
use ethers::types::{H160, Bytes};
use fnv::FnvBuildHasher;
use petgraph::{
    dot::Dot, 
    graph::DiGraph,
    visit::{IntoEdgeReferences, EdgeRef}
};
use revm::{
    primitives::{Bytecode as RevmBytecode},
    interpreter::analysis::to_analysed,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

/// 表示一个合约的控制流图和执行信息
pub struct ContractCFG {
    pub address: H160,
    pub cfg_runner: CFGRunner<'static>,
    pub executed_pcs: HashSet<u16>,
}

/// 全局交易图中的节点
#[derive(Clone, Debug)]
pub struct TransactionNode {
    pub contract_address: H160,
    pub pc: u16,
    pub instruction: String,
}

/// 全局交易图中的边
#[derive(Clone, Debug)]
pub enum TransactionEdge {
    Internal(String),    // 合约内部流转，字符串表示边的类型
    External(String),    // 合约间调用，字符串表示调用类型 (CALL, DELEGATECALL等)
}

pub struct TransactionAnalyzer {
    pub trace_steps: Vec<TraceStep>,
    pub contract_addresses: HashSet<H160>,
    pub bytecode_cache: BytecodeCache,
    pub contract_cfgs: HashMap<H160, ContractCFG>,
    pub call_edges: Vec<CallEdge>,
    pub global_graph: DiGraph<TransactionNode, TransactionEdge>,
    pub node_mapping: HashMap<(H160, u16), petgraph::graph::NodeIndex>,
}

impl TransactionAnalyzer {
    pub fn new(trace_steps: Vec<TraceStep>) -> Self {
        let contract_addresses = trace::extract_contract_addresses(&trace_steps);
        let call_edges = trace::extract_call_edges(&trace_steps);
        
        Self {
            trace_steps,
            contract_addresses,
            bytecode_cache: BytecodeCache::new(),
            contract_cfgs: HashMap::new(),
            call_edges,
            global_graph: DiGraph::new(),
            node_mapping: HashMap::new(),
        }
    }
    
    pub fn from_trace_file(trace_path: &str) -> Result<Self> {
        let trace_steps = trace::parse_trace_file(trace_path)?;
        Ok(Self::new(trace_steps))
    }
    
    pub async fn fetch_bytecodes(&mut self, blockchain_service: &impl BlockchainService) -> Result<()> {
        let addresses: Vec<H160> = self.contract_addresses.iter().cloned().collect();
        self.bytecode_cache = crate::blockchain::fetch_all_bytecodes(&addresses, blockchain_service).await?;
        Ok(())
    }
    
    /// 为每个合约生成CFG
    pub fn generate_contract_cfgs(&mut self) -> Result<()> {
        // 创建空对象防止所有权问题
        let mut contract_cfgs = HashMap::new();
        
        for (address, bytecode) in &self.bytecode_cache.cache {
            let contract_cfg = self.generate_single_contract_cfg(address, bytecode)?;
            contract_cfgs.insert(*address, contract_cfg);
        }
        
        self.contract_cfgs = contract_cfgs;
        Ok(())
    }
    
    /// 为单个合约生成CFG
    fn generate_single_contract_cfg(&self, address: &H160, bytecode: &Bytes) -> Result<ContractCFG> {
        // 转换为revm需要的格式
        let contract_data = bytecode.to_vec().into();
        let bytecode_analysed = to_analysed(RevmBytecode::new_raw(contract_data));
        
        // 获取有效跳转目标
        let revm_jumptable = bytecode_analysed.legacy_jump_table()
            .ok_or_else(|| eyre!("revm字节码分析失败"))?;
            
        let mut set_all_valid_jumpdests: HashSet<u16, FnvBuildHasher> = HashSet::default();
        let slice = revm_jumptable.as_slice();
        for (byte_index, &byte) in slice.iter().enumerate() {
            for bit_index in 0..8 {
                if byte & (1 << bit_index) != 0 {
                    let pc = (byte_index * 8 + bit_index) as u16;
                    set_all_valid_jumpdests.insert(pc);
                }
            }
        }
        
        // 解析指令块
        let mut instruction_blocks = dasm::disassemble(bytecode_analysed.original_byte_slice().into());
        for block in &mut instruction_blocks {
            block.analyze_stack_info();
        }
        
        // 创建指令块映射
        let map_to_instructionblocks: BTreeMap<(u16, u16), InstructionBlock> = instruction_blocks
            .iter()
            .map(|block| ((block.start_pc, block.end_pc), block.clone()))
            .collect();
            
        // 获取该合约的执行步骤
        let filtered_steps = trace::filter_steps_by_address(&self.trace_steps, address);
        let executed_pcs = trace::get_executed_pcs(&filtered_steps);
        
        // 创建CFG
        let mut cfg_runner = CFGRunner::new(
            bytecode_analysed.original_byte_slice().into(),
            Box::leak(Box::new(map_to_instructionblocks)),
        );
        
        // 设置执行过的PC
        cfg_runner.set_executed_pcs(executed_pcs.clone());
        
        // 建立基本连接
        cfg_runner.form_basic_connections();
        
        // 移除不可达的指令块
        cfg_runner.remove_unreachable_instruction_blocks();
        
        // 解决间接跳转
        crate::cfg_gen::stack_solve::symbolic_cycle(
            &mut cfg_runner,
            &set_all_valid_jumpdests,
            false,
        );
        
        Ok(ContractCFG {
            address: *address,
            cfg_runner,
            executed_pcs,
        })
    }
    
    /// 创建全局交易图
    pub fn build_global_transaction_graph(&mut self) -> Result<()> {
        // 为每个合约的CFG中的节点创建全局图节点
        for (address, contract_cfg) in &self.contract_cfgs {
            for node in contract_cfg.cfg_runner.cfg_dag.nodes() {
                // 只添加被执行过的节点
                if contract_cfg.executed_pcs.contains(&node.0) {
                    let instruction_block = contract_cfg.cfg_runner.map_to_instructionblock.get(&node).unwrap();
                    let pc = instruction_block.start_pc;
                    
                    // 创建交易节点
                    let tx_node = TransactionNode {
                        contract_address: *address,
                        pc,
                        instruction: instruction_block.to_string(),
                    };
                    
                    // 添加到全局图
                    let node_idx = self.global_graph.add_node(tx_node);
                    self.node_mapping.insert((*address, pc), node_idx);
                }
            }
        }
        
        // 添加合约内部边
        for (address, contract_cfg) in &self.contract_cfgs {
            for edge in contract_cfg.cfg_runner.cfg_dag.all_edges() {
                let (from_node, to_node, edge_type) = edge;
                let from_pc = from_node.0;
                let to_pc = to_node.0;
                
                // 只添加两端都被执行过的边
                if contract_cfg.executed_pcs.contains(&from_pc) && contract_cfg.executed_pcs.contains(&to_pc) {
                    if let (Some(from_idx), Some(to_idx)) = (
                        self.node_mapping.get(&(*address, from_pc)),
                        self.node_mapping.get(&(*address, to_pc))
                    ) {
                        // 添加内部边
                        let edge_label = format!("{:?}", edge_type);
                        self.global_graph.add_edge(
                            *from_idx,
                            *to_idx,
                            TransactionEdge::Internal(edge_label),
                        );
                    }
                }
            }
        }
        
        // 添加合约间调用边
        for edge in &self.call_edges {
            if let (Some(from_idx), Some(to_idx)) = (
                self.node_mapping.get(&(edge.from_addr, edge.from_pc)),
                // 假设目标合约的入口PC为0
                self.node_mapping.get(&(edge.to_addr, 0))
            ) {
                // 添加外部调用边
                self.global_graph.add_edge(
                    *from_idx,
                    *to_idx,
                    TransactionEdge::External(edge.call_type.clone()),
                );
            }
        }
        
        Ok(())
    }
    
    /// 将全局交易图导出为dot格式
    pub fn export_global_graph_dot(&self) -> String {
        let mut dot_str = String::new();
        
        writeln!(&mut dot_str, "digraph G {{").unwrap();
        writeln!(&mut dot_str, "    rankdir=TB;").unwrap();
        writeln!(&mut dot_str, "    node [shape=box, style=\"filled, rounded\", color=\"#565f89\", fontcolor=\"#c0caf5\", fontname=\"Helvetica\", fillcolor=\"#24283b\"];").unwrap();
        writeln!(&mut dot_str, "    edge [color=\"#414868\", fontcolor=\"#c0caf5\", fontname=\"Helvetica\"];").unwrap();
        writeln!(&mut dot_str, "    bgcolor=\"#1a1b26\";").unwrap();
        
        // 添加节点
        for (idx, node) in self.global_graph.node_indices().zip(self.global_graph.node_weights()) {
            let addr_str = format!("{:?}", node.contract_address);
            let label = format!("{}\\nPC: {}\\n{}", addr_str, node.pc, node.instruction.replace('"', "\\\""));
            writeln!(&mut dot_str, "    {} [label=\"{}\"];", idx.index(), label).unwrap();
        }
        
        // 添加边
        for edge in self.global_graph.edge_references() {
            let (from, to) = (edge.source().index(), edge.target().index());
            
            match &edge.weight() {
                TransactionEdge::Internal(edge_type) => {
                    let style = match edge_type.as_str() {
                        "ConditionTrue" => "color=\"#9ece6a\", label=\"True\"",
                        "ConditionFalse" => "color=\"#f7768e\", label=\"False\"",
                        "SymbolicJump" => "color=\"#e0af68\", style=\"dotted\", label=\"Symbolic\"",
                        _ => "color=\"#414868\""
                    };
                    writeln!(&mut dot_str, "    {} -> {} [{}];", from, to, style).unwrap();
                },
                TransactionEdge::External(call_type) => {
                    let style = "color=\"#7aa2f7\", style=\"bold\", penwidth=2, label=\"".to_owned() + call_type + "\"";
                    writeln!(&mut dot_str, "    {} -> {} [{}];", from, to, style).unwrap();
                }
            }
        }
        
        writeln!(&mut dot_str, "}}").unwrap();
        
        dot_str
    }
    
    /// 保存全局交易图为dot文件
    pub fn save_global_graph_dot(&self, output_path: &str) -> Result<()> {
        let dot_str = self.export_global_graph_dot();
        std::fs::write(output_path, dot_str)?;
        Ok(())
    }
    
    /// 转换为其他格式（如PNG、SVG等）
    pub fn convert_to_image(&self, dot_path: &str, output_path: &str) -> Result<()> {
        let ext = Path::new(output_path).extension().and_then(|s| s.to_str()).unwrap_or("png");
        
        let output = std::process::Command::new("dot")
            .arg(format!("-T{}", ext))
            .arg("-o")
            .arg(output_path)
            .arg(dot_path)
            .output()?;
            
        if !output.status.success() {
            return Err(eyre!("转换失败: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }
}
