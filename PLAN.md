# PLAN · M2 开源首发

## 步骤

1. **更名**（代码层）
   - `Cargo.toml`: `name = "saicord"`
   - 窗口类统一 `Saicord*` 前缀；日志 `F:\tmp\saicord.log`
   - 安装到 `F:\tools\saicord\saicord.exe`，开始菜单 `saicord.lnk`
2. **构建与实机验证**
   - `cargo build --release`
   - 框选 600×400 → 开始 → F9 → 桌面出片
   - `ffprobe` 校验 + 抽帧查边框像素
3. **文档**
   - README / AGENTS / PROJECT_CONSTITUTION / TRACK_TREE / SPEC / PLAN / progress / HANDOFF
   - `docs/adr/0001-capture-stack.md`
   - `design-lock.json`、`LICENSE`、`CHANGELOG.md`
4. **仓库**
   - `gh repo create nakamotosai/saicord --public`
   - push main，打 tag `v0.1.0`，建 release
   - 加 Gitea 远端并推送
5. **收口**
   - 刷新 `F:\projects\INDEX.md` 行
   - 项目本卡 upsert（stage=active）
   - `CLOSEOUT.md` + `progress.md`

## 验证方式

每步都有可观测产物：构建产物、mp4、`gh repo view` 输出、`git remote -v`、看板回读。
