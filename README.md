# EVM 交易流程可视化引擎 (EVM Transaction Flow Visualizer)

这个工具可以通过分析以太坊交易踪迹文件，自动生成整个交易执行过程的控制流图（CFG），包括跨合约调用的完整执行路径。

此工具是对原有 [evm-cfg](https://github.com/plotchy/evm-cfg) 和 [evm-cfg-execpath](https://github.com/Avery76/evm-cfg-execpath) 的重大升级，从单一合约路径分析器升级为全交易流程可视化引擎。

## 功能特点

- 自动解析交易踪迹，识别所有被调用的合约地址
- 通过配置的RPC节点自动获取合约字节码
- 为每个合约生成内部控制流图，并高亮实际执行路径
- 将所有局部路径图按照调用关系拼接成完整的全局执行图
- 支持识别CALL, DELEGATECALL, STATICCALL等跨合约调用
- 美观的图形输出，支持导出为DOT格式或直接渲染为图片

## 安装

1. 确保你已安装Rust和Cargo
2. 克隆仓库并编译：

```bash
git clone https://github.com/yourusername/evm-cfg-execpath.git
cd evm-cfg-execpath
cargo build --release
```

## 配置

在项目根目录创建一个`.env`文件，配置RPC节点URL：

```
GETH_API=https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY
```

你可以使用Infura、Alchemy或其他以太坊RPC提供商。

## 使用方法

基本用法：

```bash
./target/release/evm-cfg --trace <PATH_TO_TRACE_FILE> --output <OUTPUT_DOT_FILE>
```

参数说明：

- `--trace`: 交易踪迹文件路径，包含debug_traceTransaction的JSON输出
- `--output`: 输出的DOT文件路径
- `--render`: (可选) 是否自动渲染为图片格式，默认为false
- `--format`: (可选) 输出图片格式，仅在render=true时有效，默认为svg

示例：

```bash
./target/release/evm-cfg --trace ./traces/my_transaction.json --output ./output/transaction_graph.dot --render --format png
```

## 获取交易踪迹

你可以使用以下方法获取交易踪迹：

1. 使用geth调试API：

```bash
curl -X POST --data '{"jsonrpc":"2.0","method":"debug_traceTransaction","params":["0xYOUR_TX_HASH", {"tracer": "callTracer"}],"id":1}' -H "Content-Type: application/json" http://localhost:8545
```

2. 或者使用etherscan等区块浏览器提供的API

将获取到的JSON保存为文件，即可作为本工具的输入。

## 查看结果

生成的DOT文件可以使用Graphviz工具查看：

```bash
dot -Tsvg output/transaction_graph.dot -o output/transaction_graph.svg
```

或者直接使用工具的`--render`选项自动生成图片。

---

## 历史版本特性 (evm-cfg-execpath)

以下是此工具基于的原始版本中的功能，现已扩展：

### 静态分析阶段

- 通过静态分析将字节码分割成可由图表示的结构
- 提供连续的指令块，不会被跳转中断
- 提供跳转表（jumpdests位置）
- 提供所有块的入口节点
- 识别"直接"跳转（当push直接跟随jump时）
- 静态分析解开一些间接（但具体的）位置，如果push仍在同一指令块中找到
- 提供块内堆栈使用信息

### 基本边形成

直接push值的Jumps和Jumpis与其jumpdests相连。Jumpis的False侧与下一条指令相连。

### 修剪CFG

没有入边且不以JUMPDEST开始的节点无法进入，将被移除以减少混乱。

### 符号堆栈和遍历

该方法用于防止循环无限进行：
- 仅执行特定操作码：_AND_, PUSH, JUMP, JUMPI, JUMPDEST, RETURN, INVALID, SELFDESTRUCT, STOP
- 仅跟踪符号堆栈上可能的跳转位置值
- 防止遍历器进入堆栈不够大的块
- 遍历到新块时，将(current_pc, next_pc, symbolic_stack)添加到已访问节点集合

## 贡献

欢迎提交Pull Request或Issues！

## 许可

[MIT License](LICENSE)
- Highlighting all blocks and edges that were actually executed during the transaction, based on the trace.

This allows users to visualize not just the static structure of the contract, but also the precise path taken during a specific transaction.
>>>>>>> ebdb665 (Initial commit)
