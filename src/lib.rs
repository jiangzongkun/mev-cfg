#[derive(Debug, Clone, Copy, Default)]
pub struct OutputHandler {
    pub show_timings: bool,
    pub show_jump_dests: bool,
    pub show_bare_nodes: bool,
    pub show_basic_connections: bool,
}

impl OutputHandler {
    pub fn new(
        show_timings: bool,
        show_jump_dests: bool,
        show_bare_nodes: bool,
        show_basic_connections: bool,
    ) -> Self {
        Self {
            show_timings,
            show_jump_dests,
            show_bare_nodes,
            show_basic_connections,
        }
    }
}

// Export all new modules
pub mod analyzer;
pub mod blockchain;
pub mod config;
pub mod cfg_gen;
