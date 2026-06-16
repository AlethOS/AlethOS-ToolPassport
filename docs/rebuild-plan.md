# AlethOS ToolPassport 重建计划

本文用于把当前仓库归档为旧版本后，重新搭建一个可验证、可维护、可演示的
ToolPassport 项目。目标不是否定已有工作，而是保留已经做出来的模块，把环境、测试、
数据边界、真实审计路径和测试网上链流程重新打牢。

## 1. 重建原则

1. 先保命，再扩展。先处理泄露、部署、钱包、环境和测试问题，再继续做新功能。
2. 保留可信核心。Rust Trust Core 已经有较多测试覆盖，优先保留并收紧边界。
3. 切开演示和真实能力。mock、fixture、preview 可以保留，但不能冒充真实审计结果。
4. 每一步都能验收。每个阶段都要有明确命令，不能只靠“看起来能跑”。
5. 所有链上动作都人工批准。部署、签名、广播和重试都不能自动发生。

## 2. 当前旧版本归档

旧版本先作为可追溯参考保留，不继续在上面直接堆功能。

归档动作：

- 提交当前泄露止血：从 Git 跟踪中移除 `design-qa.md`，并把它加入 `.gitignore`。
- 保留当前本地审计报告：`.codex/reports/repo-risk-audit-2026-06-16.md`。
- 给当前状态开一个 PR，标题说明这是旧版本归档和重建计划。
- 不在这个 PR 里做大规模代码修复，避免把归档和重构混在一起。

人工确认：

- 检查 `design-qa.md` 是否含真实凭据；如有，先去服务商轮换或吊销。
- 确认 GitHub 部署环境 `aliyun-ecs` 是否有人工审批保护。
- 修复本机 Foundry 权限问题，确保合约检查能重新运行。

## 3. 阶段一：重建开发环境

目标：任何人拉下仓库后，都能知道需要安装什么、怎么检查、哪里不能自动执行。

要做：

- 写一份新的环境清单，列出 Rust、Python、Node、Foundry、SQLite、GitHub CLI 的版本。
- 把 `.env.example` 按用途分区：本地后端、orchestrator、dashboard、Sepolia。
- 明确 `.env` 永远不读、不提交、不打印。
- 修复 `scripts/check_contracts.sh` 的本机运行问题。
- 增加 `scripts/doctor.sh` 的检查项：Foundry 可写目录、Node 版本、Python venv、SQLite DB 路径、GitHub CLI 登录状态。
- 增加 `scripts/clean_local.sh`，默认只 dry-run 清理 ignored 缓存和本地 DB。

验收：

```bash
scripts/doctor.sh
scripts/check_docs.sh
scripts/check_schemas.sh
scripts/check_contracts.sh
```

完成标准：

- 新环境能跑完 doctor。
- 合约检查不再因为权限问题失败。
- 文档说明清楚哪些命令会联网、写缓存或需要人工批准。

## 4. 阶段二：清理仓库和发布流程

目标：仓库不再混入本地记录、生成物、误导性 demo 或不受控部署。

要做：

- 关闭或收紧 `.github/workflows/deploy.yml` 的自动部署入口。
- 默认只允许手动部署，且必须使用受保护 GitHub environment。
- 清理 `scripts/deploy.sh` 中重复的 Python 安装段。
- 引入脱敏 secret 扫描，例如 `gitleaks --redact`。
- 把 mock/demo 命令统一改名，避免用户误认为是真实审计。
- 更新 README 的“当前可用能力”和“尚未完成能力”。

验收：

```bash
scripts/check_docs.sh
scripts/check_schemas.sh
git diff --check
```

完成标准：

- CI 不会自动部署生产或测试服务器，除非人工批准。
- 本地生成物不会出现在 `git status` 的可提交文件里。
- 新人能从 README 看懂哪些能力是真实的，哪些只是 fixture。

## 5. 阶段三：重建可信后端

目标：保留 Rust Trust Core 的强项，把它变成所有真实数据和最终结论的唯一入口。

优先保留：

- Tool Registry 和 Run binding。
- append-only Run Event 和事件哈希链。
- Evidence、Artifact、路径隔离和内容 hash。
- frozen Evidence Board、Check Results、Passport、Provenance。
- Approval 和独立 Attestation Receipt。

要补强：

- 把调查进程锁从内存 `HashSet` 升级成数据库 lease，避免服务重启或多实例重复调查。
- 给所有 Trust Core-owned 事件建立更清楚的拒绝测试。
- 把错误响应统一成稳定格式，前端和 orchestrator 不解析自由文本。
- 增加 backend-only 端到端测试：Tool -> Run -> Evidence -> Board -> Check Results -> Passport -> offchain approval。

验收：

```bash
scripts/check_backend.sh
```

完成标准：

- 后端可以独立完成无链上写入的本地可信闭环。
- 重启 backend 不会造成同一个 Run 多个调查进程同时写事件。

## 6. 阶段四：重建 orchestrator

目标：把 mock 调查图和真实调查图硬分开。

要做：

- 保留 `offline_fixture_graph`，只给测试和离线 demo 用。
- 新建 `live_audit_graph`，只从 Rust Run 快照启动。
- live 模式禁止静默回退 mock evidence、mock gaps、mock findings。
- 新增 Evidence Mapping 节点：把真实来源片段映射到 claim 和 check。
- GLM 输出必须先过 Pydantic/schema 验证，再提交 Rust。
- 调研失败时输出有限结论和缺口，不伪装成完整审计。

验收：

```bash
scripts/check_orchestrator.sh
```

完成标准：

- mock 测试仍能跑。
- live 流程在没有证据或模型失败时停在明确缺口，不产生假通过结论。
- 所有冻结、评分、Passport 失败都 fail-closed。

## 7. 阶段五：重建 dashboard

目标：前端只展示后端权威数据，只发起用户操作，不复制业务规则。

要做：

- 把服务端后端地址从 `NEXT_PUBLIC_BACKEND_URL` 改为 `TRUST_CORE_URL`。
- 保留同源 `/api/trust-core/*` 代理，浏览器不直接访问 Rust 后端。
- 所有 preview、mock、fixture 数据必须有可见标识。
- 审批页面必须显示 Tool、Run、Passport hash、Audit log hash、Evidence manifest hash、chain ID、Registry。
- 失败和空状态要清楚说明，不回退到假数据。

验收：

```bash
scripts/check_dashboard.sh
```

完成标准：

- Dashboard 不显示未冻结的权威评分或 hash。
- Sepolia 提交按钮只在已有有效批准后出现。
- 页面不会暴露 RPC URL 或私钥。

## 8. 阶段六：重建合约和测试网上链

目标：先确保本地合约检查稳定，再做 Sepolia 手工验收。

要做：

- 修复 Foundry 本地环境。
- 跑通 `forge fmt --check`、`forge build`、`forge test -vvv`。
- 确认合约仍只保存最小 hash commitment。
- 使用测试网专用钱包，不复用主网钱包。
- 部署和 attestation 都只通过人工批准执行。
- 失败交易不自动重试。

验收：

```bash
scripts/check_contracts.sh
```

手工 Sepolia 验收：

1. 人工准备 `RPC_URL`、`PRIVATE_KEY`、`REGISTRY_CONTRACT`。
2. Dashboard 查看 preflight，只展示公开字段。
3. 人工批准 Sepolia attestation。
4. 后端广播一次交易。
5. 保存独立 Attestation Receipt。

完成标准：

- 合约检查稳定通过。
- Sepolia 上链只发生在人工批准后。
- Receipt 与 Passport 保持分离。

## 9. 阶段七：重建本地端到端演示

目标：用一个命令证明核心路径能复现。

要做：

- 实现 `scripts/run_demo_local.sh`，启动 backend、dashboard 和 orchestrator。
- 实现 `scripts/check_e2e_local.sh`，执行无链上写入的完整闭环。
- 本地 demo 默认不读取钱包、不部署、不广播交易。
- 输出清楚的 Demo walkthrough。

验收：

```bash
scripts/check_e2e_local.sh
scripts/check_all.sh
```

完成标准：

- 新环境可以复现 Tool 创建、Run 调查、Evidence 保存、Board freeze、Check Results、Passport freeze、offchain approval。
- 测试网上链是单独人工步骤，不混入普通 CI。

## 10. 推荐提交顺序

1. 旧版本归档与重建计划。
2. 环境 doctor、clean 脚本和 Foundry 检查修复。
3. CI/CD 收紧和部署脚本清理。
4. 后端 investigation lease 和 backend-only 端到端测试。
5. orchestrator mock/live 拆分。
6. Evidence Mapping 和真实有限结论。
7. Dashboard 配置名、审批展示和 preview 边界清理。
8. 本地端到端 demo。
9. Sepolia 手工验收文档更新。

## 11. 不做的事

- 不接主网。
- 不自动部署合约。
- 不自动广播链上交易。
- 不把 Codex、dashboard 或 orchestrator 变成数据库或链上写入权威。
- 不把 mock 结果包装成真实审计结论。
- 不为了“看起来干净”重写已经有测试保护的 Rust 核心。

## 12. 最终目标

重建完成后，本项目应该满足三件事：

1. 普通开发者能按文档搭好环境并跑完检查。
2. 用户能看到一条可复查的真实本地审计路径。
3. 测试网上链只作为人工批准后的最后证明步骤，而不是隐藏副作用。
