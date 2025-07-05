# EVM Transaction Flow Visualizer

This tool analyzes Ethereum transaction trace files to automatically generate control flow graphs (CFGs) for the entire transaction execution process, including cross-contract call paths.

This tool is a major upgrade to the original [evm-cfg](https://github.com/plotchy/evm-cfg) and [evm-cfg-execpath](https://github.com/Avery76/evm-cfg-execpath), evolving from a single-contract path analyzer to a complete transaction flow visualization engine.

## Features

- Automatically parses transaction traces and identifies all contract addresses involved
- Fetches contract bytecode through configured RPC nodes
- Generates internal control flow graphs for each contract with highlighted execution paths
- Combines all local path graphs into a complete global execution graph based on call relationships
- Supports identification of CALL, DELEGATECALL, STATICCALL and other cross-contract calls
- Highlights all nodes containing SSTORE operations (state modifications)
- Provides aesthetically pleasing graph output, with export to DOT format or direct rendering to images

## Installation

1. Ensure you have Rust and Cargo installed
2. Clone the repository and build:

```bash
git clone https://github.com/yourusername/evm-cfg-execpath.git
cd evm-cfg-execpath
cargo build --release
```

## Configuration

Create a `.env` file in the project root directory and configure your RPC node URL:

```
GETH_API=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY
```

You can use Infura, Alchemy, or other Ethereum RPC providers.

## Usage

Basic usage:

```bash
# Method 1: Provide an existing trace file
./target/release/evm-cfg --trace <PATH_TO_TRACE_FILE> --output <OUTPUT_DOT_FILE>

# Method 2: Directly provide a transaction hash (automatic trace retrieval)
./target/release/evm-cfg --tx-hash <TRANSACTION_HASH>
```

Parameters:

- `--trace`: Path to transaction trace file containing JSON output from debug_traceTransaction
- `--tx-hash`: Transaction hash value; the program will automatically retrieve the trace and generate the graph
- `--output`: (Optional) Path for the output DOT file; if not provided, named after the transaction hash
- `--render`: (Optional) Whether to automatically render to an image format, default is false
- `--format`: (Optional) Output image format, only valid when render=true, default is svg

Examples:

```bash
# Using an existing trace file
./target/release/evm-cfg --trace ./traces/my_transaction.json --output ./output/transaction_graph.dot --render --format png

# Directly using a transaction hash (automatic processing)
./target/release/evm-cfg --tx-hash 0xef39c19ceb07373914204e76019943d57e5c4e99760ec2a337a6e9d38a315fbc --render
```

## Obtaining Transaction Traces

You can obtain transaction traces using the following methods:

1. Using the geth debug API:

```bash
curl -X POST --data '{"jsonrpc":"2.0","method":"debug_traceTransaction","params":["0xYOUR_TX_HASH", {"tracer": "callTracer"}],"id":1}' -H "Content-Type: application/json" http://localhost:8545
```

2. Or using APIs provided by block explorers like Etherscan

Save the retrieved JSON as a file to use as input for this tool.

## Viewing Results

The generated DOT files can be viewed using Graphviz tools:

```bash
dot -Tsvg output/transaction_graph.dot -o output/transaction_graph.svg
```

Or directly use the tool's `--render` option to automatically generate images.

## Graph Styles and Color Codes

The generated control flow graphs use the following color conventions:

- **Green nodes**: Represent nodes that were actually executed
- **Purple nodes**: Represent nodes containing SSTORE opcodes (state modifications)
- **Green edges**: Represent execution paths
- **Blue bold edges**: Represent cross-contract calls
- **Red edges**: Represent conditional branches (false)
- **Green edges**: Represent conditional branches (true)

The SSTORE opcode is responsible for modifying contract storage state in the Ethereum EVM. By highlighting these nodes in purple, you can quickly identify all operations that change on-chain state during a transaction.

---

## Legacy Features (evm-cfg-execpath)

The following are features from the original version this tool is based on, now extended:

### Static Analysis Phase

- Splits bytecode into a structure representable by a graph through static analysis
- Provides continuous instruction blocks that aren't interrupted by jumps
- Provides a jump table (jumpdests positions)
- Provides entry nodes for all blocks
- Identifies "direct" jumps (when push is directly followed by jump)
- Static analysis resolves some indirect (but concrete) positions if the push is still found in the same instruction block
- Provides stack usage information within blocks

### Basic Edge Formation

Jumps and Jumpis with direct push values are connected to their jumpdests. The False side of Jumpis is connected to the next instruction.

### Pruning the CFG

Nodes with no incoming edges and not starting with JUMPDEST cannot be entered and will be removed to reduce clutter.

### Symbolic Stack and Traversal

The method used to prevent loops from running indefinitely:
- Only execute specific opcodes: _AND_, PUSH, JUMP, JUMPI, JUMPDEST, RETURN, INVALID, SELFDESTRUCT, STOP
- Only track possible jump destination values on the symbolic stack
- Prevent the traverser from entering blocks where the stack is not large enough
- When traversing to a new block, add (current_pc, next_pc, symbolic_stack) to the set of visited nodes

## Contributing

Pull Requests or Issues are welcome!

## License

[MIT License](LICENSE)

- Highlighting all blocks and edges that were actually executed during the transaction, based on the trace.

This allows users to visualize not just the static structure of the contract, but also the precise path taken during a specific transaction.
