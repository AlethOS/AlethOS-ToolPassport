# AlethOS ToolPassport 技术文档

## 1. 架构原则

AlethOS ToolPassport 是一个独立 monorepo，也是未来 AlethOS 平台中的可集成模块。仓库内各层职责固定：

| 层 | 技术 | 唯一职责 |
| --- | --- | --- |
| Dashboard | Next.js、React、Tailwind、React Flow、TanStack Query | 创建任务和展示状态，不承载业务逻辑 |
| Trust Core | Rust、Axum、SQLx、SQLite、Alloy | API、持久化、事件、产物、哈希、权限和链上写入 |
| Orchestrator | Python、LangGraph、Pydantic | 长程任务图、状态、分支、重试和人工确认 |
| Reasoning | GLM-5.1 | 规划、证据抽取、评分、自审和报告生成 |
| Research | GitHub API/MCP、Context7、受控网页读取 | 只读调研和证据采集 |
| Web3 | Solidity、Foundry | 最小 Passport Registry |
| Engineering | Codex | 开发期实现、测试和修复，不属于产品运行时信任链 |

关键约束：

- Rust 是系统记录与可信计算的唯一权威来源。
- LangGraph 不直接写数据库、计算最终哈希或提交链上交易，只调用 Rust API。
- Dashboard 不直接调用 GLM、数据库或链上 RPC。
- GLM 输出必须经过 Pydantic/schema 验证后才能进入后续节点。
- 链上写入必须由 Rust 执行，并且需要显式人工批准。

## 2. Monorepo 结构

```text
.
├── AGENTS.md
├── README.md
├── .env.example
├── docs/
│   ├── project-overview.md
│   ├── technical-design.md
│   └── work-guide.md
├── backend/
│   ├── Cargo.toml
│   ├── migrations/
│   ├── src/
│   └── tests/
├── orchestrator/
│   ├── pyproject.toml
│   ├── src/toolpassport_orchestrator/
│   └── tests/
├── dashboard/
│   ├── package.json
│   ├── app/
│   ├── components/
│   └── lib/
├── contracts/
│   ├── foundry.toml
│   ├── src/
│   ├── script/
│   └── test/
├── schemas/
│   ├── run-event.schema.json
│   ├── node-result.schema.json
│   ├── evidence.schema.json
│   └── passport.schema.json
├── examples/
├── scripts/
└── runs/
```

`runs/` 仅用于本地生成物，不作为数据库替代；其中的敏感或大体积产物默认不提交。

跨模块共享类型以 `schemas/` 中的 JSON Schema 为机器可读权威来源。本文档解释其语义；字段变更必须同时更新 schema、实现和测试。

## 3. 系统数据流

```text
Dashboard
  -> POST /api/runs
Rust Backend
  -> 创建 run，启动 orchestrator subprocess/HTTP task
LangGraph
  -> 通过后端 API 写 node event、evidence 和 artifact
Rust Backend
  -> 持久化并通过 SSE 推送事件
Dashboard
  -> GET /api/runs/:id + SSE /api/runs/:id/events
LangGraph
  -> 请求生成 Passport
Rust Backend
  -> 校验、规范化、保存、计算 hash
Dashboard
  -> 用户批准 attestation
Rust Backend
  -> 提交测试网交易并保存 receipt
```

首版进程模型使用：Rust 后端启动一个受控的 Python orchestrator 子进程，并通过 `RUN_ID` 与 `BACKEND_URL` 关联。后续可替换为独立服务，不改变 API 合约。

## 4. LangGraph 审计图

```text
clarify_goal
  -> plan_audit
  -> research_tool
       -> web_research
       -> github_research
       -> docs_research
  -> extract_evidence
  -> build_tool_card
  -> score_tool
  -> self_review
       -> continue_research  (证据不足，最多 2 轮)
       -> repair_json        (结构输出无效，最多 2 次)
       -> generate_passport
  -> generate_report
  -> request_human_approval
       -> attest_onchain
       -> finish_without_attestation
  -> finish
```

每个 node 必须：

- 使用类型化输入/输出；
- 开始和结束时向后端写事件；
- 将可复查的中间结果保存为 artifact；
- 失败时记录可操作错误；
- 遵守固定重试上限；
- 不自行执行钱包签名或未知 Shell。

## 5. 共享状态与输出协议

对应机器可读契约：

- `schemas/run-event.schema.json`
- `schemas/node-result.schema.json`
- `schemas/evidence.schema.json`
- `schemas/passport.schema.json`

### Graph State

```json
{
  "run_id": "uuid",
  "goal": "string",
  "tool": {
    "name": "string",
    "type": "string",
    "urls": []
  },
  "current_node": "string",
  "research_round": 0,
  "evidence_ids": [],
  "artifact_ids": [],
  "errors": [],
  "approval_status": "not_requested"
}
```

### Node Result

```json
{
  "run_id": "uuid",
  "node_id": "string",
  "status": "success",
  "summary": "string",
  "artifact_ids": [],
  "errors": [],
  "next": "node_id"
}
```

允许状态：`pending | running | success | failed | waiting_approval | cancelled`。

### Run Event

```json
{
  "event_id": "uuid",
  "run_id": "uuid",
  "node_id": "string",
  "event_type": "node_started",
  "payload": {},
  "created_at": "RFC3339 timestamp"
}
```

事件类型：

```text
run_created
run_status_changed
node_started
node_finished
artifact_created
evidence_created
approval_required
approval_resolved
attestation_submitted
attestation_confirmed
error
```

事件只追加，不更新历史事件。

## 6. Rust 后端

### 模块边界

```text
api/          Axum handlers，只做协议转换和鉴权
domain/       Run、Evidence、Passport、Artifact、Attestation 类型
services/     业务规则、哈希、审批、attestation
repository/   SQLx 持久化
events/       append-only event 和 SSE
artifacts/    文件写入、读取和路径隔离
web3/         Alloy client 和 Registry 调用
```

### 最小 API

| 方法 | 路径 | 行为 |
| --- | --- | --- |
| `POST` | `/api/runs` | 创建并启动审计 |
| `GET` | `/api/runs` | 返回任务摘要列表 |
| `GET` | `/api/runs/:run_id` | 返回任务、当前节点和产物摘要 |
| `POST` | `/api/runs/:run_id/cancel` | 请求取消任务 |
| `GET` | `/api/runs/:run_id/events` | SSE 事件流 |
| `POST` | `/api/runs/:run_id/events` | orchestrator 追加事件 |
| `POST` | `/api/runs/:run_id/evidence` | 保存证据 |
| `POST` | `/api/runs/:run_id/artifacts` | 保存中间或最终产物 |
| `GET` | `/api/passports/:passport_id` | 返回结构化 Passport |
| `POST` | `/api/runs/:run_id/approval` | 批准或拒绝链上写入 |
| `POST` | `/api/runs/:run_id/attest` | 在批准后提交测试网交易 |

所有响应使用 JSON；SSE 除外。错误响应至少包含：

```json
{
  "code": "stable_machine_code",
  "message": "human readable summary",
  "details": {}
}
```

### SQLite 表

```text
runs
run_events
evidence
artifacts
passports
approvals
attestations
```

最低要求：

- 所有表使用 UUID 主键；
- 事件、证据和产物关联 `run_id`；
- `run_events` 只追加；
- Passport 保存 schema 版本和规范化 JSON；
- secrets 不进入任何表；
- migration 由 SQLx 管理。

## 7. Passport 数据契约

Passport 顶层结构：

```json
{
  "passport_version": "0.1",
  "tool": {},
  "audit": {},
  "capability_claims": [],
  "interfaces": [],
  "evidence": {
    "items": [],
    "missing_evidence": []
  },
  "scores": {
    "capability_clarity": {"score": 0, "reason": ""},
    "interface_openness": {"score": 0, "reason": ""},
    "automation_readiness": {"score": 0, "reason": ""},
    "data_portability": {"score": 0, "reason": ""},
    "permission_risk": {"score": 0, "reason": ""},
    "evidence_quality": {"score": 0, "reason": ""},
    "ecosystem_fit": {"score": 0, "reason": ""},
    "total_score": 0,
    "rating": "trial"
  },
  "recommendation": {},
  "web3_attestation": {}
}
```

规则：

- 每个能力声明、风险和评分理由应引用 evidence ID；
- 每项评分为 `0-5` 的整数；
- `total_score` 由 Rust 计算，不能信任 LLM 提供值；
- rating 由 Rust 根据总分映射；
- `passport_version` 变更时必须保留向后读取能力。

## 8. 证据模型

每条证据至少包含：

```json
{
  "evidence_id": "uuid",
  "source_type": "official_docs",
  "source_url": "https://...",
  "title": "string",
  "excerpt": "short supported excerpt or summary",
  "retrieved_at": "RFC3339 timestamp",
  "supports": ["capability_claim_id"],
  "metadata": {}
}
```

允许的首版来源：用户材料、官网、官方文档、GitHub README、repo 元数据、公开示例。

调研规则：

- 记录来源 URL 和抓取时间；
- 区分官方声明、源码信号和第三方反馈；
- 不执行被审计项目；
- 不把搜索摘要当成最终证据；
- 证据冲突时保留双方并标记冲突。

## 9. 哈希与 Artifact

Rust 负责最终哈希。

1. Passport 在哈希前转为规范化 JSON：UTF-8、稳定 key 顺序、无无意义空白。
2. Audit Log 使用按事件创建时间和 event ID 排序后的稳定 JSON。
3. 使用 SHA-256，API 和链上调用统一表示为 `0x` 前缀的 32-byte hex。
4. Attestation 后不得静默修改已签名产物；修改必须生成新 Passport 版本和新 Hash。

Artifact 文件必须限制在配置的 workspace 目录内，禁止路径穿越。

## 10. Web3 层

`ToolPassportRegistry` 仅提供：

```solidity
recordPassport(
    string toolId,
    string toolType,
    bytes32 passportHash,
    bytes32 auditLogHash
)
```

约束：

- Solidity `^0.8.20`；
- 不做 upgradeability；
- 不保存完整报告；
- 不接主网；
- 部署和写入都需要人工确认；
- 私钥只从环境变量或外部签名器读取；
- 后端保存 transaction hash、chain ID、contract address 和 receipt 状态。

## 11. Dashboard

首版页面使用单一 Run workspace：

```text
左侧：Run 列表
中间：React Flow 审计图
右侧：当前节点、证据、评分、报告、链上证明
底部：Run Event 时间线
```

必须展示：

- `run_id`、工具名、总状态、当前节点；
- 节点的 `pending/running/success/failed/waiting_approval` 状态；
- Evidence 来源和缺失证据；
- 七维评分与理由；
- Passport 与 Markdown 报告；
- Passport Hash、Audit Log Hash、Registry 和交易 Hash；
- 批准/拒绝 attestation 操作。

前端只通过后端 API 和 SSE 获取数据。

## 12. 环境变量

```env
APP_ENV=development
BACKEND_HOST=127.0.0.1
BACKEND_PORT=8080
DATABASE_URL=sqlite://data/toolpassport.db
ARTIFACT_ROOT=./runs

ORCHESTRATOR_COMMAND=python
ORCHESTRATOR_BACKEND_URL=http://127.0.0.1:8080

ZAI_API_KEY=
ZAI_BASE_URL=https://api.z.ai/api/paas/v4
ZAI_MODEL=glm-5.1

CHAIN_ID=
RPC_URL=
PRIVATE_KEY=
REGISTRY_CONTRACT=

NEXT_PUBLIC_BACKEND_URL=http://127.0.0.1:8080
```

`.env` 不提交。前端公开变量不得包含密钥。

## 13. 安全边界

```yaml
permissions:
  network_read: allow_for_research
  local_file_read: user_provided_or_workspace_only
  local_file_write: workspace_only
  shell_execution: deny_for_audited_tools
  wallet_sign: human_approval_required
  contract_deploy: human_approval_required
  mainnet: deny
  paid_api: budget_limited
  secrets_access: env_only
```

额外要求：

- URL loader 必须限制协议、响应大小、超时和重定向；
- 防止 SSRF 访问本机、内网和云 metadata 地址；
- 日志必须脱敏；
- orchestrator 子进程使用最小环境变量；
- approval 必须绑定 run、hash、chain 和 contract；
- 失败的链上交易不自动重发。

## 14. 测试与验收

### Backend

- API happy path 与错误响应；
- append-only event 顺序；
- run 状态迁移；
- Passport schema、总分和 rating 计算；
- 规范化哈希稳定性；
- 路径穿越、secret 脱敏和 approval 检查；
- mock RPC 的 attestation 流程。

### Orchestrator

- 每个 node 的类型化输入输出；
- weak evidence 分支与最大研究轮数；
- invalid JSON 修复与重试上限；
- node 失败事件；
- approval 等待与拒绝分支；
- mock GLM 的完整图运行。

### Dashboard

- Run Graph 状态映射；
- SSE 更新；
- Evidence、评分、报告和交易信息显示；
- approval 操作；
- loading、empty 和 failed 状态。

### Contracts

- record 和读取 Passport；
- event 参数；
- 多条记录；
- Foundry formatting、build 和 tests。

### 端到端

- mock 模式完成一次无需网络和钱包的完整审计；
- 真实模式完成一个工具审计；
- 人工批准后完成一次测试网 attestation；
- Dashboard 可展示完整过程。
