use crate::cfg_gen::dasm::*; 
use itertools::Itertools; // Contains many useful collection operations, such as sorting, grouping, etc.
use lazy_static::lazy_static; // Allows us to define "global variables" that are initialized only once and can be used later.
use petgraph::dot::Dot;
use petgraph::prelude::*;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use super::BLOCK_ENDERS_U8;

lazy_static! {
    pub static ref TOKYO_NIGHT_COLORS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("red", "#f7768e");
        m.insert("orange", "#ff9e64");
        m.insert("yellow", "#e0af68");
        m.insert("green", "#9ece6a");
        m.insert("cyan", "#73daca");
        m.insert("teal", "#2ac3de");
        m.insert("darkblue", "#7aa2f7");
        m.insert("purple", "#bb9af7");
        m.insert("bg", "#1a1b26");
        m.insert("font", "#c0caf5");
        m.insert("deepred", "#703440");
        m
    };
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edges {
    Jump,           // Next instruction in sequence
    ConditionTrue,  // Conditional jumpi, true branch
    ConditionFalse, // Conditional jumpi, false branch
    SymbolicJump,   // Jump to a symbolic value
} // Defines different types of edges in the control flow graph

impl Debug for Edges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Edges::Jump => write!(f, ""),
            Edges::ConditionTrue => write!(f, "True"),
            Edges::ConditionFalse => write!(f, "False"),
            Edges::SymbolicJump => write!(f, "Symbolic"),
        }
    }
} // Defines how each edge type is displayed when printed.

type CFGDag = GraphMap<(u16, u16), Edges, Directed>; // Defines a directed graph type CFGDag

pub struct CFGRunner<'a> {
    pub cfg_dag: CFGDag,
    pub last_node: Option<(u16, u16)>,
    pub jumpi_edge: Option<Edges>, // Records the last node and jumpi edge type
    // These two fields are used to track the state of the CFG
    pub bytecode: Vec<u8>, // Stores the entire contract bytecode
    pub map_to_instructionblock: &'a BTreeMap<(u16, u16), InstructionBlock>, // This mapping maps (start_pc, end_pc) to instruction blocks
    pub executed_pcs: Option<HashSet<u16>>, // New: records executed PCs
} // Defines the CFGRunner struct, which contains the DAG of the control flow graph, the last node, jumpi edge, bytecode, and mapping to instruction blocks.

impl<'main> CFGRunner<'main> {
    pub fn new( // Constructor, accepts bytecode and instruction block mapping
        bytecode: Vec<u8>, 
        map_to_instructionblock: &'main BTreeMap<(u16, u16), InstructionBlock>, // Pass in the mapping of bytecode to instruction blocks
    ) -> Self { // Return a new CFGRunner instance
        // Initialize control flow graph
        let mut cfg_dag: CFGDag = GraphMap::new(); // Create a new control flow graph

        for keys in map_to_instructionblock
            .keys() // Iterate through the keys of instruction blocks
            .sorted_by(|a, b| a.0.cmp(&b.0)) // Sort by start_pc
        {
            cfg_dag.add_node(*keys); // Add each (start_pc, end_pc) node to the graph
        } // Add all (start_pc, end_pc) nodes to the graph

        Self {
            cfg_dag,
            last_node: None, // Initialize the last node as None
            jumpi_edge: None,
            bytecode,
            map_to_instructionblock,
            executed_pcs: None, // Initialize the new field as None
        } // Return a new CFGRunner instance
    }

    pub fn initialize_cfg_with_instruction_blocks(
        &mut self, 
        instruction_blocks: Vec<InstructionBlock>, // Accept a vector of instruction blocks
        // This vector contains all instruction blocks, each with start_pc and end_pc
        // as well as corresponding opcodes and stack information
    ) -> eyre::Result<()> {
        for block in instruction_blocks {
            self.cfg_dag.add_node((block.start_pc, block.end_pc));
        } // Add each instruction block's (start_pc, end_pc) to the control flow graph
        Ok(())
    } // This function initializes the control flow graph by adding the given instruction blocks to the graph.

    pub fn form_basic_connections(&mut self) {
        /*
        There are 4 cases of edges that we can connect from basic static analysis:
        1. Jumpi false
        2. Jumpi true (direct jump)
        3. Jump (direct jump)
        4. Block ender is a basic instruction (connect to next pc)
            - this happens when a block is broken up by a jumpdest
        */

        // get last pc in bytecode, this is done by iterating over the instruction blocks and finding the largest end_pc
        let last_pc_total = self
            .map_to_instructionblock
            .iter()
            .map(|((_entry_pc, _exit_pc), instruction_block)| instruction_block.end_pc)
            .max()
            .unwrap(); // Get the last pc in the bytecode, which is the maximum end_pc of all instruction blocks

        // We need to iterate over each of the nodes in the graph, and check the end_pc of the (start_pc, end_pc) node
        for ((_entry_pc, _exit_pc), instruction_block) in self.map_to_instructionblock.iter() {
            let end_pc = instruction_block.end_pc;
            let start_pc = instruction_block.start_pc;
            let last_op = instruction_block.ops.last().unwrap();
            let _last_op_pc = last_op.0;
            let last_op_code = last_op.1;

            let direct_push = &instruction_block.stack_info.push_used_for_jump;
            let direct_push_val = direct_push.as_ref().copied(); 

            // Case 1: Jumpi false
            if last_op_code == 0x57 {
                // Jumpi false
                let next_pc = end_pc + 1;
                if next_pc >= last_pc_total {
                    // continue;
                } else {
                    let next_node = self.get_node_from_pc(next_pc);
                    self.cfg_dag
                        .add_edge((start_pc, end_pc), next_node, Edges::ConditionFalse);
                }
            }
            if instruction_block.indirect_jump.is_none() && direct_push_val.is_some() {
                // we know this is a direct jump
                // Case 2: Direct Jumpi true
                if last_op_code == 0x57 {
                    // Jumpi true
                    let next_pc = format!("{}", direct_push_val.unwrap())
                        .parse::<u16>()
                        .unwrap(); // this is so stupid but its only done once
                    let next_node = self.get_node_from_pc(next_pc);
                    self.cfg_dag
                        .add_edge((start_pc, end_pc), next_node, Edges::ConditionTrue);
                } 

                // Case 3: Direct Jump
                if last_op_code == 0x56 {
                    // Jump
                    let next_pc = format!("{}", direct_push_val.unwrap())
                        .parse::<u16>()
                        .unwrap(); // this is so stupid but its only done once
                    let next_node = self.get_node_from_pc(next_pc);
                    self.cfg_dag
                        .add_edge((start_pc, end_pc), next_node, Edges::Jump);
                }
            }

            if !BLOCK_ENDERS_U8.contains(&last_op_code)
                && super::opcode(last_op_code).name != "unknown"
            {
                // Block ender is a basic instruction, but not exiting
                let next_pc = end_pc + 1;

                if next_pc >= last_pc_total {
                    continue;
                }
                // println!("next_pc: {}, last_pc_total: {}", next_pc, last_pc_total);

                let next_node = self.get_node_from_pc(next_pc);
                self.cfg_dag
                    .add_edge((start_pc, end_pc), next_node, Edges::Jump);
            } 
        }
    } 

    pub fn remove_unreachable_instruction_blocks(&mut self) {
        // We need to iterate over the nodes in self.map_to_instructionblock, and remove any that have no incoming/outgoing edges and do not begin with a jumpdest
        let mut to_remove: Vec<(u16, u16)> = Vec::new();
        for ((_entry_pc, _exit_pc), instruction_block) in self.map_to_instructionblock.iter() {
            let start_pc = instruction_block.start_pc;
            let end_pc = instruction_block.end_pc;
            let incoming_edges = self
                .cfg_dag
                .edges_directed((start_pc, end_pc), Direction::Incoming);
            if incoming_edges.count() == 0 {
                // This node has no incoming edges, so it is unreachable
                if instruction_block.ops[0].1 != 0x5b && start_pc != 0 {
                    // This node does not begin with a jumpdest, so it is unreachable
                    to_remove.push((start_pc, end_pc));
                }
            }
        }

        // remove the found nodes from the cfg and from the self.map_to_instructionblock
        for node in to_remove {
            self.cfg_dag.remove_node(node);
        }
    }

    pub fn get_node_from_pc(&self, pc: u16) -> (u16, u16) {
        for (_key, val) in self.map_to_instructionblock.iter() {
            if val
                .ops
                .iter()
                .map(|(instruction_pc, _op, _push_val)| *instruction_pc == pc)
                .any(|x| x)
            {
                return (val.start_pc, val.end_pc);
            }
        }
        panic!("Could not find node for pc {pc}, hex: {:x}", pc);
    }

    pub fn get_node_from_entry_pc(&self, pc: u16) -> (u16, u16) {
        for (key, val) in self.map_to_instructionblock.iter() {
            if key.0 == pc {
                return (val.start_pc, val.end_pc);
            }
        }
        panic!("Could not find node for entry pc {pc}");
    }

    pub fn get_node_from_exit_pc(&self, pc: u16) -> (u16, u16) {
        for (key, val) in self.map_to_instructionblock.iter() {
            if key.1 == pc {
                return (val.start_pc, val.end_pc);
            }
        }
        panic!("Could not find node for exit pc {pc}");
    }

    pub fn cfg_dot_str_with_blocks(&mut self) -> String {
        /*
        digraph {
            node [shape=box, style=rounded, color="#565f89", fontcolor="#c0caf5", fontname="Helvetica"];
            edge [color="#565f89", fontcolor="#c0caf5", fontname="Helvetica"];
            bgcolor="#1a1b26";
            0 [ label = "pc0: PUSH1 0x80"]
            1 [ label = "pc2: JUMP" color = "red"]
            ...
        }
        */

        // have to use the petgraph module as the node indexes and edges are not the same as our weights
        let mut dot_str = Vec::new();
        let raw_start_str = r##"digraph G {
    node [shape=box, style="filled, rounded", color="#565f89", fontcolor="#c0caf5", fontname="Helvetica", fillcolor="#24283b"];
    edge [color="#414868", fontcolor="#c0caf5", fontname="Helvetica"];
    bgcolor="#1a1b26";"##; 
        dot_str.push(raw_start_str.to_string()); 

        let nodes_and_edges_str = format!(
            "{:?}",
            Dot::with_attr_getters(
                &self.cfg_dag,
                &[
                    petgraph::dot::Config::GraphContentOnly,
                    petgraph::dot::Config::NodeNoLabel,
                    petgraph::dot::Config::EdgeNoLabel
                ],
                &|_graph, edge_ref| {
                    let (from, to, edge_type) = edge_ref;
                    // Check if both from and to nodes are highlighted
                    let highlight = if let Some(ref pcs) = self.executed_pcs {
                        let from_block = self.map_to_instructionblock.get(&from).unwrap();
                        let to_block = self.map_to_instructionblock.get(&to).unwrap();
                        pcs.contains(&from_block.start_pc) && pcs.contains(&to_block.start_pc)
                    } else {
                        false
                    };
                    if highlight {
                        // Highlight edge (green)
                        format!(
                            "label = \"{:?}\" color = \"{}\" penwidth=3",
                            edge_type,
                            TOKYO_NIGHT_COLORS.get("green").unwrap()
                        )
                    } else {
                        // Original logic
                        match edge_type {
                            Edges::Jump => "".to_string(),
                            Edges::ConditionTrue => format!(
                                "label = \"{:?}\" color = \"{}\"",
                                edge_type,
                                TOKYO_NIGHT_COLORS.get("green").unwrap()
                            ),
                            Edges::ConditionFalse => format!(
                                "label = \"{:?}\" color = \"{}\"",
                                edge_type,
                                TOKYO_NIGHT_COLORS.get("red").unwrap()
                            ),
                            Edges::SymbolicJump => format!(
                                "label = \"{:?}\" color = \"{}\", style=\"dotted, bold\"",
                                edge_type,
                                TOKYO_NIGHT_COLORS.get("yellow").unwrap()
                            ),
                        }
                    }
                },
                &|_graph, (_id, node_ref)| {
                    let mut node_str = String::new();
                    let instruction_block = self.map_to_instructionblock.get(node_ref).unwrap();
                    let color = instruction_block.node_color();
                    match color {
                        Some(color) => {
                            node_str.push_str(&format!(
                                "label = \"{instruction_block}\" color = \"{color}\""
                            ));
                        }
                        None => {
                            node_str.push_str(&format!("label = \"{instruction_block}\""));
                        }
                    }
                    // if the node has no incoming edges, fill the node with deepred
                    if instruction_block.start_pc == 0 {
                        node_str.push_str(" shape = invhouse");
                    } else if self.cfg_dag.neighbors_directed(*node_ref, Incoming).count() == 0 {
                        node_str.push_str(&format!(
                            " fillcolor = \"{}\"",
                            TOKYO_NIGHT_COLORS.get("deepred").unwrap()
                        ));
                    }
                    // New code: If the node has been executed, add highlight color
                    if let Some(ref pcs) = self.executed_pcs {
                        if pcs.contains(&instruction_block.start_pc) {
                            node_str.push_str(&format!(
                                " fillcolor = \"{}\" fontcolor = \"#1a1b26\"",
                                TOKYO_NIGHT_COLORS.get("green").unwrap()
                            ));
                        }
                    }
                    node_str
                }
            )
        );
        dot_str.push(nodes_and_edges_str);
        let raw_end_str = r#"}"#;
        dot_str.push(raw_end_str.to_string());
        dot_str.join("\n")
    }

    /// Export only highlighted nodes and edges (only executed parts)
    pub fn cfg_dot_str_highlighted_only(&self) -> String {
        let mut dot_str = Vec::new();
        let raw_start_str = r##"digraph G {
    node [shape=box, style="filled, rounded", color="#565f89", fontcolor="#1a1b26", fontname="Helvetica"];
    edge [color="#9ece6a", fontcolor="#1a1b26", fontname="Helvetica", penwidth=3];
    bgcolor="#1a1b26";"##;
        dot_str.push(raw_start_str.to_string());

        // Only output highlighted nodes
        if let Some(ref pcs) = self.executed_pcs {
            for ((start_pc, end_pc), block) in self.map_to_instructionblock.iter() {
                if pcs.contains(start_pc) {
                    let label = format!("{}", block);
                    // Color priority: SSTORE > ADD/SUB > others
                    let mut has_sstore = false;
                    let mut has_add_or_sub = false;
                    for (_pc, op, _push) in &block.ops {
                        let opname = super::opcode(*op).name.to_ascii_lowercase();
                        if opname == "sstore" {
                            has_sstore = true;
                            break;
                        }
                        if opname == "add" || opname == "sub" {
                            has_add_or_sub = true;
                        }
                    }
                    let fillcolor = if has_sstore {
                        "#f7768e" // Pink for SSTORE
                    } else if has_add_or_sub {
                        "#ff9e64" // Orange for ADD/SUB
                    } else {
                        "#9ece6a" // Green for others
                    };
                    let mut attrs = vec![
                        format!("label = \"{}\"", label.replace("\"", "\\\"")),
                        format!("fillcolor = \"{}\" fontcolor = \"#1a1b26\"", fillcolor)
                    ];
                    if *start_pc == 0 {
                        attrs.push("shape = invhouse".to_string());
                    }
                    dot_str.push(format!(
                        "\"{}_{}\" [{}];",
                        start_pc, end_pc,
                        attrs.join(" ")
                    ));
                }
            }

            // Only output highlighted edges (from and to both highlighted)
            for (from, to, _edge_type) in self.cfg_dag.all_edges() {
                if pcs.contains(&from.0) && pcs.contains(&to.0) {
                    dot_str.push(format!(
                        "\"{}_{}\" -> \"{}_{}\";",
                        from.0, from.1, to.0, to.1
                    ));
                }
            }
        }

        dot_str.push("}".to_string());
        dot_str.join("\n")
    }

    pub fn set_executed_pcs(&mut self, pcs: HashSet<u16>) {
        self.executed_pcs = Some(pcs);
    }
}
