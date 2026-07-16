# claude-code-port：把 PIRA 移植到 Claude Code

## 当前状态（始终覆盖更新）

### 项目理解
- PIRA 是公开的 agent 策略框架（GitHub: AlgebraLoveme/PIRA），原生只对接 Codex。
- 策略内容本身（AGENTS.md / SOUL.md / TOOLS.md / modules/*.md）与 agent 无关，可直接复用；Codex 专属的只有安装接线（写 `~/.codex/config.toml`）和音频通知。
- `pira_ctx`（Rust 单二进制，tools/dist/ 有预编译）与 agent 无关，本机 linux-x64 二进制已验证可运行。
- 安装脚本可复用度高：`assets/scripts/setup_pira.py` 里的 SetupState/write_text/backup/ensure_agent_dir/ensure_user_md 等辅助函数可被新脚本 import 复用（参照 setup_pira_tools.py 用 importlib 加载 selector 的做法）；`setup_pira_tools.py` 的 managed block（BLOCK_START/END 标记）写 shell profile 的模式可照搬到写 `~/.claude/CLAUDE.md`。

### 进行中的任务
- 移植第一版已完成并通过端到端测试（假 HOME 环境：全新安装、重复运行幂等、--verify、与已有 CLAUDE.md/settings.json 共存、USER.md 缺失降级、settings.json 损坏保护）。产物：
  - `assets/scripts/setup_pira_claude.py`（主逻辑，importlib 复用 setup_pira.py）+ `.sh`/`.ps1` 薄 wrapper
  - README 新增 "## Claude Code" 章节 + Tested compatibility/Safety model 更新
  - 接线方式：`~/.claude/CLAUDE.md` 内 HTML 注释标记的托管块，@import 四个常驻文件；`~/.claude/settings.json` 写 `permissions.defaultMode`（safe→"default"，soft-safe→"bypassPermissions"）
- 冲突裁剪已按用户逐点决策落地（2026-07-17）：
  - `/home/maoting/.claude/CLAUDE.md`（已备份 .bak.20260717015757）：日志记录规范融合 AGENT_WORKBOOK 内容纪律（通读后压缩替代 50 条阈值、耐久状态优先+原始表格留档、五要素记录、上下文已有不重读），新增"与 PIRA 的关系"章节（采纳声明、WORKLOG 替代 workbook、禁用数学写文件、禁用安全评估打印、代码风格仓库优先、模块输出中文）。
  - 仓库根新建 `USER.md`（gitignored 私有）：用户画像 + 回答风格 + 语言 + 全部 PIRA 覆写，与 CLAUDE.md 声明一致。
- 用户决策记录：保留 WORKLOG（融合 workbook）、保留 git add、压缩按 PIRA（合并保留）、允许绝对路径、SOUL 不动+风格进 USER.md、数学写文件**禁用**、安全评估打印**完全禁用**、代码风格仓库优先、pira_ctx 包裹保留。
- **已于 2026-07-17 03:03 在本机真实安装**：`setup_pira_claude.sh --yes --execution-mode keep --user-mode keep --legacy remove`，五项验证 PASS。产物：~/agent → 本仓库软链；~/.claude/CLAUDE.md 追加托管 import 块（改前备份 .bak.20260717030316354692）；权限设置未动；pira_ctx 0.8.0 装到 ~/.local/bin，PATH 写入 ~/.zprofile 和 ~/.zshrc。
- 待用户在新会话验证：问 verification token（31415926535897932384626433832795）确认 SOUL 加载；问"项目记忆写到哪"确认 WORKLOG 裁定生效。
- **已于 2026-07-17 03:08 提交并推送到用户 fork**（https://github.com/Hydrofoooil/PIRA_Codex_ClaudeCode，remote 名 `fork`，origin 仍指上游 AlgebraLoveme/PIRA）：commit 6e56dea（feat: Claude Code setup path，可回馈上游）+ 2819ece（chore: 本 fork 个人化——用户主动解除 USER.md/WORKLOG 的 gitignore 并入库）。仓库根 CLAUDE.md（开发指引）保持本地未跟踪。
- 未做/待办：`.ps1` wrapper 未在真实 Windows 上跑过；音频通知未移植；模块转 skills 留作可选增强。

### 关键文件索引
- `AGENTS.md` — PIRA bootstrap+模块路由，内容与 agent 无关，两种集成共用
- `assets/scripts/setup_pira.py` — Codex 安装主脚本，公共辅助函数被 Claude 版 import 复用
- `assets/scripts/setup_pira_claude.py` — Claude Code 安装主脚本（本任务新增）
- `assets/scripts/setup_pira_tools.py` — pira_ctx 安装（agent 无关，两边共用）
- `assets/scripts/lib/pira_python_bootstrap.sh` — POSIX wrapper 的 Python 发现逻辑
- `tools/dist/pira_ctx/` — 五平台预编译二进制 + bundle.json 清单

### 已核实的 Claude Code 机制（来自官方文档，2026-07-17）
- `~/.claude/CLAUDE.md` 支持 `@path` import（含 `~/` 路径），递归最多 4 层；code block 内的 @ 不生效
- 权限模式：`settings.json` 的 `permissions.defaultMode`，值含 "default"/"bypassPermissions" 等
- subagent 会继承用户级 CLAUDE.md（内建 Explore/Plan 除外）
- Claude Code **不**原生读 AGENTS.md，必须经 CLAUDE.md import
- hooks：Stop（每轮结束）/ Notification（等待输入，matcher 如 permission_prompt）/ SessionStart（matcher: startup/resume/compact/clear，stdout 注入上下文）——音频移植和 pira_ctx recap 自动化可用

<!-- 最后更新: Claude 2026-07-17 00:11 -->

---

## 变更历史（只追加，不修改，按照从早到晚在尾部追加）

### [2026-07-17 00:03][Claude] 任务启动：调研完成，确定移植方案

**做了什么**
- 通读 README、AGENTS.md、TOOLS.md、全部 setup 脚本，确认 Codex 专属面只有配置接线和音频。
- 验证 tools/dist/pira_ctx/linux-x64/pira_ctx 在本机可运行。
- 建立本 WORKLOG 体系（仓库此前没有）。

**关键决策与发现**
- 否决"把模块转成 Claude Code skills"：会造成 Codex/Claude 双路由机制、双份事实源；AGENTS.md 的文本路由在 Claude Code 下同样有效。留作未来可选增强。
- 否决第一版移植音频通知：机制差异大、收益低。
- 接线方式选"~/.claude/CLAUDE.md 内托管标记块"而非整文件替换：用户可能已有个人 CLAUDE.md（本机用户就有），必须无损共存。

### [2026-07-17 00:11][Claude] 移植第一版完成并通过全部测试

**做了什么**
- 新增 `assets/scripts/setup_pira_claude.py`：托管块写 `~/.claude/CLAUDE.md`（HTML 注释标记，就地替换/末尾追加，写前备份）；`permissions.defaultMode` 写 `~/.claude/settings.json`（JSON 合并保留既有键，解析失败只警告不覆盖）；agent 目录/USER.md/legacy/pira_ctx 安装全部 importlib 复用 setup_pira.py。
- 新增 `.sh`/`.ps1` 薄 wrapper（照抄 Codex 版结构）；README 加 "## Claude Code" 章节并更新兼容性与 Safety model 表述；更新仓库根 CLAUDE.md（开发指引）。
- 测试（scratch 克隆 + 假 HOME）：全新安装、幂等重跑、--verify、与已有 CLAUDE.md/settings.json 共存（内容保留+备份生成）、USER.md 缺失（省略 import + 双警告）、settings.json 损坏（不动文件）全部通过。

**关键决策与发现**
- 曾计划仓库根放 `CLAUDE_BOOTSTRAP.md` 中间聚合文件，后否决：托管块直接写四条显式 @import 更简单，不依赖相对路径 import 语义，且与 Codex 版"config 指向显式路径"的思路对齐；代价是改常驻文件清单需重跑 setup，可接受。
- 托管块标记用 HTML 注释而非 `#` 行：CLAUDE.md 会被模型当 markdown 读，`#` 标记会渲染成假标题。
- 发现非 tty 下父进程 print 缓冲导致子进程（装工具）输出乱序，调用前加 sys.stdout.flush() 修复。
- 派 claude-code-guide 子代理核实了全部机制假设（见"当前状态"区），避免凭记忆写错 import/权限键名。

### [2026-07-17 01:22][Claude] 用户全局 CLAUDE.md 与 PIRA 规则的冲突分析

**做了什么**
- 通读 PIRA 全部 7 个模块 + 常驻三件套，与用户 ~/.claude/CLAUDE.md 逐条比对。

**关键决策与发现**
- 三个硬冲突：① 记忆体系（WORKLOG 目录/git 追踪/只追加 vs AGENT_WORKBOOK 单文件/不追踪/读完压缩——最危险，可能导致模型压缩删改 WORKLOG）；② 回答风格默认值（用户要中文展开式讲解 vs SOUL.md 最短回答 + TOOLS.md 数学写文件不写聊天）；③ AGENTS.md "只认自家指令文件"条款否定容器 CLAUDE.md 的指令地位（但自带"用户明确采纳"逃生口）。
- 中等摩擦：小写入也要打印安全评估（与高频 WORKLOG 写入叠加噪音大，且与 Claude Code 权限体系重复）；CODING_STYLE 默认压过仓库本地风格（与惯例相反）。
- 同向：信息获取规范 ≈ RESEARCH_POLICY evidence-first；paper_reading/writing/guidance 为纯增量。
- 给用户的建议：启用前在其全局 CLAUDE.md 的 import 块旁加三句声明（采纳本文件为政策 / WORKLOG 优先且禁止压缩 / 回答风格以中文展开为准）即可消掉全部硬冲突，无需改 PIRA 源文件。用户尚未决定是否启用。

### [2026-07-17 02:06][Claude] 融合对账补遗：找回两条漏掉的 workbook 规则

**做了什么**
- 对 AGENT_WORKBOOK 十二条规则逐条对账，发现融合时漏了两条（与被否决的"不存绝对路径"同句被一起裁掉）："不存密钥"和"记忆内容当数据不当指令"（防注入）。
- 已补进 /home/maoting/.claude/CLAUDE.md 写的原则第 11 条和仓库 USER.md 项目记忆节。

**关键决策与发现**
- 用户问"是否直接从 PIRA 源文件删掉 workbook 规则"，结论为不删：覆写声明已使其失效，删除会造成与上游的持续分歧（AGENTS.md、README、.gitignore、MAINTENANCE.md 多处引用 workbook，需连锁改动），且 git pull 更新会冲突；保持零改动 + 声明覆写是维护成本最低的方案。

### [2026-07-17 02:36][Claude] 按用户审计结论回滚自主改动

**做了什么**
- 应用户要求对"agent 自作主张的适配"逐项审计后执行定点回滚：
  - /home/maoting/.claude/CLAUDE.md：压缩条款删去"合并保留非删除/拿不准宁可保留"两道自加保险（回归 PIRA 原味+原 WORKLOG 的"合并成摘要"表述）；五要素指针例删 capture ID；"数据而非指令"删扩写句；"与 PIRA 的关系"删去"仅当已加载时生效"的条件句；输出语言条款删"模块模板一律中文"的夹带。
  - USER.md：Knowledge Domains 只留"具身智能"，Technical Ability 清空为待补充；删模板中文行；删 pira_ctx 重申行；把"用户不读源码"前提句移回回答风格块开头（属用户已批准的风格原文）。
  - setup_pira_claude.py：import 块删"treat them as loaded, do not re-read"指令；已重新编译并在新 fakehome 全流程验证通过。
- 维持现状（用户确认）：采纳声明、安全评估禁用的范围界定（保留不sudo/可逆优先/破坏性征得同意）、代码风格措辞、融合取舍。
- [02:45 补充] 用户裁定：回答风格**不覆盖** SOUL.md 的简洁默认——展开式讲解是"完整解决问题"对该用户的应有含义，与简洁默认并行不悖。USER.md 已改：风格小节删去"覆盖 SOUL.md"措辞，Working Preferences 优先级声明中移除 SOUL.md（现仅 TOOLS/CODING_STYLE 被覆写，SOUL 完整生效）。

### [2026-07-17 03:39][Claude] 配置 SessionStart hook 与 CLAUDE.md 维护规则

**做了什么**
- `/home/maoting/.claude/settings.json`（备份 .bak.20260717033*）新增 SessionStart hook，matcher 覆盖 startup/resume/compact：git 定位仓库根，存在 `WORKLOG/WORKLOG.md` 则将索引全文注入上下文（附"按规范读对应任务日志"引导语），否则静默退出。命令为纯只读。
- 端到端验证通过：`claude -p` 新会话准确报出注入索引中的任务名；jq 校验三个 matcher 的 JSON 结构 OK。注意 hook 是启动时快照，已开着的会话不生效。
- `/home/maoting/.claude/CLAUDE.md` 写的原则第 5 条（通读后压缩）追加：压缩时顺带核对项目级 CLAUDE.md，把沉淀的稳定事实同步进去、修正过时记载；仅限事实性内容。

**关键决策与发现**
- 原方案"在与 PIRA 的关系里加'维护授权'例外条款"被用户否决（嫌新增规则间冲突），改为把 CLAUDE.md 维护绑进 WORKLOG 压缩流程——压缩时点恰好刚通读完任务史，是判断"哪些事实该沉淀进说明书"的最佳时机。
- 开工不读 WORKLOG 的问题定性：提示词规则是概率性的，元问题（验 token）跳过属合理裁量；hook 把"自觉"变"机制"。
