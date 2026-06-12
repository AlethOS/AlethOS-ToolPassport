# AlethOS ToolPassport 项目说明

## 1. 项目定位

AlethOS ToolPassport: Verifiable AI Tool Audit Module

AlethOS ToolPassport 是 AlethOS 体系中的 AI Tool 审计与证明模块。它由 GLM-5.1 驱动、LangGraph 编排、Rust 承载可信执行底座，并通过链上 Passport Registry 提供可验证证明。

本项目不是完整 AlethOS，也不负责 AlethOS 的其他控制平面能力。ToolPassport 面向广义 AI Tool 生态，包括 Agent 框架、MCP Server、CLI、API 服务、工作流平台和 AI 应用，并为未来 AlethOS 平台提供可复用的工具审计、Passport 和链上证明能力。

## 2. 要解决的问题

AI Tool 数量快速增长，但用户、开发者和团队很难可靠判断：

- 工具真正具备哪些能力，哪些只是宣传声明；
- 是否有清晰、稳定、可自动化调用的接口；
- 是否存在密钥、文件、Shell、钱包或数据泄露风险；
- 数据和工作流是否可迁移；
- 文档、源码、测试和运行证据是否充分；
- 是否值得接入长期工作流。

AlethOS ToolPassport 将一次性的人工判断转化为可记录、可复查、可验证的长程审计任务。

## 3. 核心产物

### AI Tool Passport

每个被审计工具都会生成一份结构化 Passport，包含：

- 工具身份、类别和来源；
- 经证据支持的能力声明；
- 接口与自动化适配程度；
- 权限、数据和集成风险；
- 七个审计维度的评分与理由；
- 推荐用途、限制和人工检查项；
- Passport Hash、Audit Log Hash 和链上证明。

### Audit Run Log

每次审计都会生成完整运行记录，包含：

- 用户目标、模型和工作流版本；
- LangGraph 节点状态和时间线；
- 调研来源、工具调用和中间产物；
- 错误、重试、人工确认和修复过程；
- 最终 Passport、报告、哈希和交易记录。

### Onchain Attestation

链上只记录最小摘要：

- `toolId`
- `toolType`
- `passportHash`
- `auditLogHash`
- `auditor`
- `timestamp`

完整报告和证据不写入链上。

## 4. 目标用户

- **AI 工具重度使用者**：快速判断工具是否可信、可用和可长期依赖。
- **Agent 应用开发者**：评估工具的集成可行性、自动化能力和替代风险。
- **团队工具治理者**：形成可复用、可审查的工具准入记录。
- **Web3 AI 生态参与者**：复用开放的审计证明和未来的工具声誉数据。

## 5. MVP 用户流程

1. 用户在 Dashboard 创建审计任务，提供工具名称、URL、GitHub 或本地材料。
2. LangGraph 澄清目标并生成审计计划。
3. 调研节点读取网页、文档和 GitHub 元数据，不安装或执行未知项目。
4. GLM-5.1 抽取证据、生成 Tool Card、评分并自审。
5. 证据不足时继续调研；输出异常时进入 JSON 修复节点。
6. Rust 后端持久化所有事件、证据、产物和状态。
7. 系统生成 Passport JSON、Markdown 报告和 Audit Run Log。
8. Rust 计算稳定哈希。
9. 用户人工确认后，将摘要写入测试网 Registry。
10. Dashboard 展示任务图、证据、评分、报告和交易 Hash。

## 6. 审计维度

每项评分为 `0-5`，每项必须附带基于证据的理由。缺少证据时必须降分。

| 维度 | 关注点 |
| --- | --- |
| Capability Clarity | 能力、使用场景和边界是否清楚 |
| Interface Openness | API、CLI、SDK、MCP、本地运行和集成文档 |
| Automation Readiness | 稳定输入输出、脚本化、无界面运行、日志与错误处理 |
| Data Portability | 数据与配置导出、开放格式和平台锁定 |
| Permission Risk | 文件、Shell、密钥、钱包、Cookie 和付费 API 权限 |
| Evidence Quality | 官方文档、源码、示例、测试、更新记录和可复现性 |
| Ecosystem Fit | 可组合性、可替换性及对 Agent 工作流的适配程度 |

总分：

```text
total_score = average(dimension_scores) * 20
```

| 总分 | 接入建议 |
| --- | --- |
| 0-20 | 不建议接入 |
| 21-40 | 仅适合人工辅助使用 |
| 41-60 | 可谨慎试用 |
| 61-80 | 适合低风险工作流接入 |
| 81-100 | 核心工具或协议组件候选 |

## 7. 黑客松 MVP 范围

### 必须完成

- GLM-5.1 参与规划、证据抽取、评分、自审和报告生成；
- LangGraph 执行可恢复的长程审计图；
- Rust Axum 后端提供 Run、Evidence、Passport、Artifact 和 Attestation 能力；
- SQLite 持久化任务状态、事件、证据和结果；
- Next.js Dashboard 展示 Run Graph、实时节点状态、证据、评分和链上证明；
- Solidity `ToolPassportRegistry` 合约与 Foundry 测试；
- 测试网人工确认后写入 Passport Hash；
- 至少生成两份 AI Tool Passport；
- README、Demo walkthrough 和 3-5 分钟视频。

### 明确不做

- 不做完整 Marketplace、账号系统或复杂声誉网络；
- 不做多 Agent 自由协作系统；
- 不自动安装、构建或执行未知 GitHub 项目；
- 不做大规模爬虫；
- 不接主网；
- 不在前端、数据库、日志或仓库中保存明文密钥；
- 不允许无人确认的钱包签名或链上写入；
- 不让 Codex 在产品运行时自由修改生产代码。

## 8. 核心 Demo

推荐审计对象：

1. Hermes Agent；
2. LangGraph 或 OpenClaw；
3. 可选一个 MCP Server。

Demo 必须清楚展示：

- 创建任务与 GLM-5.1 审计计划；
- LangGraph 节点实时流转与至少一次分支；
- GitHub/文档证据及缺失证据；
- 七维评分、Passport 和报告；
- Rust 生成的 Run Log 与哈希；
- 人工确认；
- 测试网交易 Hash 和 Explorer 页面。

## 9. 成功标准

项目达到以下状态即可提交：

- 一个命令可启动本地依赖和各服务；
- Dashboard 可创建并观察完整审计任务；
- 中断后任务状态可恢复或明确失败；
- Passport JSON 可通过 schema 校验；
- Run Log 可完整复查节点、证据和错误；
- 相同规范化产物可生成稳定哈希；
- Foundry 合约测试通过；
- 至少一份 Passport 的 Hash 已写入测试网；
- 两份示例审计可用于 Demo；
- 新环境可按 README 复现。

## 10. 后续方向

MVP 之后可增加多审计方结果聚合、审计方声誉、工具声誉图谱、团队准入策略、IPFS 产物存储，并作为独立模块接入完整 AlethOS 平台。
