# main infra Miri 审计证据（2026-11-06）

这是一份已纳入 Git 的历史快照，可随分支上传；生产实现没有在此次审计中修复。

- 被测生产基线：`7e124fa98f6acd4561736d4325a2edcb0f5b3de2`（当时的本地 `main`）。
- 测试与运行器提交：`ac8083de8`。
- 工具链：`nightly-2026-05-24-x86_64-unknown-linux-gnu`。
- 默认模型为 Stacked Borrows；另一组显式启用 `-Zmiri-tree-borrows`。
- 每种模型运行 113 项原有测试和 16 项新增契约测试，共 129 项。
- 首轮每例上限 60 秒；唯一超时用例延长至 180 秒后在 97.916 秒通过。

| 结果（计入延时复测） | 通过 | UB 诊断用例 | 泄漏用例 | 未决超时 |
| --- | ---: | ---: | ---: | ---: |
| Stacked Borrows | 71 | 56 | 2 | 0 |
| Tree Borrows | 78 | 49 | 2 | 0 |

失败用例数量不等于独立缺陷数量。保留原始 60 秒统计和单独的延时复测，
不覆盖最初的超时证据。借用模型差异、测试泄漏和 UB 在汇总中分别列出。

## 阅读入口

- [问题来源与复现步骤](../../MIRI_AUDIT.md)
- [按模块汇总、模型差异与全部失败诊断](summary.md)
- [机器可读汇总](summary.json)
- [Stacked Borrows 全部 129 项](stacked/results.md)
- [Tree Borrows 全部 129 项](tree/results.md)
- [宿主原有 113 项](host/results.md)
- [Tree Borrows 压力测试延时复测](tree-slow-recheck/results.md)
- [运行器通过路径](runner-passing/results.md)及[失败路径](runner-failing/results.md)
- [通过时生成 stamp、失败时不生成 stamp 的验证](validation/runner-integration.txt)
- [运行器自身的五项回归检查](validation/runner-tests.txt)
- [GN 生成](validation/gn-final.txt)、[GNRT 状态检查](validation/sync-check.txt)、[生产源码范围检查](validation/scope-check.txt)
- [归档校验清单](manifest.json)

## 日志组织与保真范围

每个验证目录包含 `results.json`、`results.md`、`discovery.txt` 和 `cases.txt`。
`cases.txt` 按结果清单中的顺序汇总逐例日志，既包含失败也包含通过的用例。
Markdown 表格链接到日志起始行；JSON 中的 `log_line` / `log_end_line`
使用从 1 开始的行号。本地阅读器若不支持 `#L` 跳转，可按测试名搜索日志段。

归档移除了 ANSI 终端颜色转义序列与行尾空白，并加上每段测试名称、状态标题
及文件结束标记（便于通过 Git 的空白检查）；
原始命令、诊断、回溯、返回码、测试状态和耗时均保留。日志中的绝对路径是
当时环境的记录，复现时需按本机调整。此处不包含编译产物、临时依赖目录、
旧轮次重复日志或补丁副本；修改本身可直接通过 Git 查看。

`manifest.json` 记录归档文件的 SHA-256，以及原始逐例日志的 SHA-256 和
归档行范围。它不包含自身的哈希。归档文件应保持为历史快照；之后的新审计
应放入另一个明确命名的目录，不覆盖这次证据。

运行器的后续输出仍写入 `out/`。原工作目录中的输出保留用于复查，审核和
分享应使用本目录，它不依赖这些未被 Git 跟踪的输出文件。
