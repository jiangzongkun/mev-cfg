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
    graph::DiGraph,
    visit::{EdgeRef}
};
use revm::{
    primitives::{Bytecode as RevmBytecode},
    interpreter::analysis::to_analysed,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

/// Represents a contract's control flow graph and execution information
pub struct ContractCFG {
    pub address: H160,
    pub cfg_runner: CFGRunner<'static>,
    pub executed_pcs: HashSet<u16>,
}

/// Node in the global transaction graph
#[derive(Clone, Debug)]
pub struct TransactionNode {
    pub contract_address: H160,
    pub pc: u16,
    pub instruction: String,
    pub contains_sstore: bool,  // New field, marks whether it contains SSTORE opcode
}

impl Default for TransactionNode {
    fn default() -> Self {
        Self {
            contract_address: H160::zero(),
            pc: 0,
            instruction: String::new(),
            contains_sstore: false,
        }
    }
}

/// Edge in the global transaction graph
#[derive(Clone, Debug)]
pub enum TransactionEdge {
    Internal(String),    // Internal contract flow, string represents edge type
    External(String),    // Cross-contract call, string represents call type (CALL, DELEGATECALL, etc.)
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
    
    /// Generate CFG for each contract
    pub fn generate_contract_cfgs(&mut self) -> Result<()> {
        // Create empty objects to prevent ownership issues
        let mut contract_cfgs = HashMap::new();
        
        for (address, bytecode) in &self.bytecode_cache.cache {
            let contract_cfg = self.generate_single_contract_cfg(address, bytecode)?;
            contract_cfgs.insert(*address, contract_cfg);
        }
        
        self.contract_cfgs = contract_cfgs;
        Ok(())
    }
    
    /// Generate CFG for a single contract
    fn generate_single_contract_cfg(&self, address: &H160, bytecode: &Bytes) -> Result<ContractCFG> {
        // Convert to the format required by revm
        let contract_data = bytecode.to_vec().into();
        let bytecode_analysed = to_analysed(RevmBytecode::new_raw(contract_data));
        
        // Get valid jump targets
        let revm_jumptable = bytecode_analysed.legacy_jump_table()
            .ok_or_else(|| eyre!("revm bytecode analysis failed"))?;
            
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
        
        // Parse instruction blocks
        let mut instruction_blocks = dasm::disassemble(bytecode_analysed.original_byte_slice().into());
        for block in &mut instruction_blocks {
            block.analyze_stack_info();
        }
        
        // Create instruction block mapping
        let map_to_instructionblocks: BTreeMap<(u16, u16), InstructionBlock> = instruction_blocks
            .iter()
            .map(|block| ((block.start_pc, block.end_pc), block.clone()))
            .collect();
            
        // Get execution steps for this contract
        let filtered_steps = trace::filter_steps_by_address(&self.trace_steps, address);
        let executed_pcs = trace::get_executed_pcs(&filtered_steps);
        
        // Create CFG
        let mut cfg_runner = CFGRunner::new(
            bytecode_analysed.original_byte_slice().into(),
            Box::leak(Box::new(map_to_instructionblocks)),
        );
        
        // Set executed PCs
        cfg_runner.set_executed_pcs(executed_pcs.clone());
        
        // Establish basic connections
        cfg_runner.form_basic_connections();
        
        // Remove unreachable instruction blocks
        cfg_runner.remove_unreachable_instruction_blocks();
        
        // Resolve indirect jumps
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
    
    /// Create global transaction graph
    pub fn build_global_transaction_graph(&mut self) -> Result<()> {
        // Create global graph nodes for each node in contract CFGs
        for (address, contract_cfg) in &self.contract_cfgs {
            for node in contract_cfg.cfg_runner.cfg_dag.nodes() {
                // Only add executed nodes
                if contract_cfg.executed_pcs.contains(&node.0) {
                    let instruction_block = contract_cfg.cfg_runner.map_to_instructionblock.get(&node).unwrap();
                    let pc = instruction_block.start_pc;
                    
                    // Check if it contains SSTORE opcode
                    let contains_sstore = instruction_block.ops.iter().any(|(_, op, _)| *op == 0x55); // SSTORE opcode is 0x55
                    
                    // Create transaction node
                    let tx_node = TransactionNode {
                        contract_address: *address,
                        pc,
                        instruction: instruction_block.to_string(),
                        contains_sstore, // Set SSTORE flag
                    };
                    
                    // Add to global graph
                    let node_idx = self.global_graph.add_node(tx_node);
                    self.node_mapping.insert((*address, pc), node_idx);
                }
            }
        }
        
        // Add internal edges
        for (address, contract_cfg) in &self.contract_cfgs {
            for edge in contract_cfg.cfg_runner.cfg_dag.all_edges() {
                let (from_node, to_node, edge_type) = edge;
                let from_pc = from_node.0;
                let to_pc = to_node.0;
                
                // Only add edges where both endpoints were executed
                if contract_cfg.executed_pcs.contains(&from_pc) && contract_cfg.executed_pcs.contains(&to_pc) {
                    if let (Some(from_idx), Some(to_idx)) = (
                        self.node_mapping.get(&(*address, from_pc)),
                        self.node_mapping.get(&(*address, to_pc))
                    ) {
                        // Add internal edge
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
        
        // Add cross-contract call edges
        for edge in &self.call_edges {
            if let (Some(from_idx), Some(to_idx)) = (
                self.node_mapping.get(&(edge.from_addr, edge.from_pc)),
                // Assume target contract's entry PC is 0
                self.node_mapping.get(&(edge.to_addr, 0))
            ) {
                // Add external call edge
                self.global_graph.add_edge(
                    *from_idx,
                    *to_idx,
                    TransactionEdge::External(edge.call_type.clone()),
                );
            }
        }
        
        Ok(())
    }
    
    /// Export global transaction graph in DOT format
    pub fn export_global_graph_dot(&self) -> String {
        let mut dot_str = String::new();
        
        writeln!(&mut dot_str, "digraph G {{").unwrap();
        writeln!(&mut dot_str, "    rankdir=TB;").unwrap();
        writeln!(&mut dot_str, "    node [shape=box, style=\"filled, rounded\", color=\"#565f89\", fontcolor=\"#c0caf5\", fontname=\"Helvetica\", fillcolor=\"#24283b\"];").unwrap();
        writeln!(&mut dot_str, "    edge [color=\"#414868\", fontcolor=\"#c0caf5\", fontname=\"Helvetica\"];").unwrap();
        writeln!(&mut dot_str, "    bgcolor=\"#1a1b26\";").unwrap();
        
        // Add nodes
        for (idx, node) in self.global_graph.node_indices().zip(self.global_graph.node_weights()) {
            let addr_str = format!("{:?}", node.contract_address);
            let label = format!("{}\\nPC: {}\\n{}", addr_str, node.pc, node.instruction.replace('"', "\\\""));
            
            // If node contains SSTORE opcode, highlight it with special style (purple)
            if node.contains_sstore {
                writeln!(
                    &mut dot_str,
                    "    {} [label=\"{}\", fillcolor=\"#bb9af7\", fontcolor=\"#1a1b26\", penwidth=2];",
                    idx.index(),
                    label
                ).unwrap();
            } else {
                writeln!(&mut dot_str, "    {} [label=\"{}\"];", idx.index(), label).unwrap();
            }
        }
        
        // Add edges
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
    
    /// Save global transaction graph to DOT file
    pub fn save_global_graph_dot(&self, output_path: &str) -> Result<()> {
        let dot_str = self.export_global_graph_dot();
        std::fs::write(output_path, dot_str)?;
        Ok(())
    }
    
    /// Convert to other formats (PNG, SVG, etc.)
    pub fn convert_to_image(&self, dot_path: &str, output_path: &str) -> Result<()> {
        let ext = Path::new(output_path).extension().and_then(|s| s.to_str()).unwrap_or("png");
        
        let output = std::process::Command::new("dot")
            .arg(format!("-T{}", ext))
            .arg("-o")
            .arg(output_path)
            .arg(dot_path)
            .output()?;
            
        if !output.status.success() {
            return Err(eyre!("Conversion failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }
}
