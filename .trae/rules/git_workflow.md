# Git 工作流规则（agent 必须遵守）

> 目的：小步提交、及时上传，让任何一次改动都可回滚，防止"越改越乱"。

## 提交与上传
- 每完成一个**逻辑上独立且验证过**的改动（修一个 bug、加一个系统、改一份资产），立即提交并推送，不要攒着：
  `git add <相关路径>` → `git commit -m "<简述>"` → `git push origin main`
- 一次提交只做一件事；提交信息简短说明改了什么。
- 不把不相关的改动混进同一提交；未经用户确认不 `git add -A` 整仓。
- 提交前尽量跑最小验证（如 `cargo check`），确认编译通过再提交。
- 禁止改写已推送的历史：不对已 push 的 commit 做 `amend` / `rebase`。

## 动手前检查
- 开始较大或风险较高的改动（重构、删代码、动公共模块/宏/资产管线）前，先 `git status -sb` 确认工作区干净、代码已推送；有未提交内容就先提交推送，再动手。

## 出错回滚
- 改动出错或越改越乱时，**优先回滚，不要继续叠补丁**：
  1. `git status -sb` + `git diff` 先确认改动范围；
  2. 未提交的改动：`git checkout -- <file>` 丢弃；
  3. 已提交未推送：`git reset --hard <上一个好 commit>`；
  4. 已推送：`git revert <坏 commit>`（保留历史）。
- 回滚后向用户说明：回滚到了哪个 commit、丢掉了哪些改动。

## 远程
- `origin` = github.com/tlqqhfhc/EmbersReborn，默认分支 `main`，直接 push。
- `upstream` = 上游 Embers 仓库，只用于拉取上游更新，不向其 push。
