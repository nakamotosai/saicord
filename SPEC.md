# SPEC · M2 开源首发 v0.1.0

## 目标

把已经跑通的录制器整理成一个可以对外发布的开源项目：名字、文档、许可、仓库全部就位，任何人在 Windows 10/11 上 `cargo build --release` 就能得到同样的东西。

## 范围

1. 项目更名 `saicord`，代码内所有标识（crate、窗口类、日志路径、安装路径、开始菜单）同步
2. 十真相源落盘：README / AGENTS / PROJECT_CONSTITUTION / TRACK_TREE / SPEC / PLAN / progress / HANDOFF / CLOSEOUT / docs/adr
3. 设计锁 `design-lock.json`（颜色、字号、圆角、缩放断言）
4. LICENSE（MIT）+ CHANGELOG
5. GitHub 公开仓 `nakamotosai/saicord`，首条 release tag `v0.1.0`
6. Gitea 镜像 `ssh://git@vps-jp-gitea/sai/saicord.git`
7. 项目本卡登记（stage=active）

## 非目标

- 不新增功能（系统声音、多屏、托盘全部推到 M3+）
- 不改交互（选区 → 开始 → F9 已经是定稿）
- 不做安装包（发布单个 exe + 源码构建说明）

## 验收条件

| # | 条件 | 判定方式 |
|---|---|---|
| A1 | `cargo build --release` 零 error | 命令输出 |
| A2 | 实机录一段 600×400 出片 | 桌面出现 mp4，`ffprobe` 分辨率/时长正确 |
| A3 | 画面内无边框像素 | 抽帧统计琥珀色像素 = 0 |
| A4 | 200% DPI 下工具条按钮可读可点 | 截图 + 实测点击命中 |
| A5 | GitHub 仓库公开可访问 | `gh repo view` 返回 public |

## 风险

- 名称冲突：GitHub 上若已有 `saicord` 仓库，改用 `saicord-rec`（先查）
- MF 编码在无独显机器上可能回退软编，帧率下降——首发文档标注
