# CLOSEOUT · M2 开源首发 v0.1.0

日期：2026-11-02

## 本轮交付

| # | 项 | 证据 |
|---|---|---|
| 1 | 更名 saicord（crate / 窗口类 / 日志 / 安装路径 / 开始菜单） | `Cargo.toml` name=saicord；`F:\tools\saicord\saicord.exe`；开始菜单 `saicord.lnk` |
| 2 | 十真相源 + 设计锁 + LICENSE + CHANGELOG | 仓库根目录 |
| 3 | GitHub 公开仓 | https://github.com/nakamotosai/saicord （visibility=PUBLIC，默认分支 main） |
| 4 | Release v0.1.0 带二进制 | asset `saicord.exe` 551,424 B |
| 5 | Gitea 镜像 | `ssh://git@vps-jp-gitea/sai/saicord.git`，main + v0.1.0 已推 |
| 6 | 项目本卡 | projects.md 新增 saicord 行，1 insertion，已回读 |
| 7 | INDEX.md | 新增 saicord 行 |

## 验收对照（SPEC A1–A5）

- A1 `cargo build --release` 零 error ✅
- A2 实机 600×400 出片 ✅（H.264 600×400 + AAC）
- A3 抽帧琥珀像素 = 0 ✅
- A4 200% DPI 工具条 472×96、字号 30px、点击命中 ✅
- A5 GitHub 公开可访问 ✅
- A6 仓库内无移植表述 ✅（`grep -ri dogsc` 源码与文档 0 命中）

## 残留

- 本机仍装着旧 Electron 版 DogSC（`F:\tools\DogSC` + 开始菜单 `DogSC.lnk`），用户未表态是否删除
- `target/` 里残留旧 crate 名的 fingerprint（gitignore，不影响发布）
- 无 CI；首次外部贡献前需要加 GitHub Actions（build + clippy）

## 下一里程碑

M3 系统声音（WASAPI loopback）——需先定「系统声 + 麦克风混音」的优先级策略。


---

# CLOSEOUT · M3 系统声音 v0.2.0

日期：2026-11-02

## 交付

| # | 项 | 证据 |
|---|---|---|
| 1 | 音频改为 WASAPI loopback | `src/audio.rs` 重写，cpal 依赖删除 |
| 2 | 工具条「系统声 开/关」 | 截图 + 实测点击 |
| 3 | 440 Hz 音源录制命中 | DFT 峰值 = 440 Hz，max_volume −20.8 dB |
| 4 | 体积下降 | 551,424 B → 498,176 B |
| 5 | 旧 Electron 版 DogSC 清除 | `F:\tools\DogSC`、开始菜单 `DogSC.lnk`、`%APPDATA%\DogSC` 全部删除 |

## 验收对照（SPEC B1–B5）

- B1 ✅ 峰值频率 440 Hz
- B2 ✅ max_volume −20.8 dB（远高于 −40 dB 阈值）
- B3 ✅ AAC 48 kHz stereo
- B4 ✅ 无设备时 `audio_cfg = None`，只录画面
- B5 ✅ 体积减少 53 KB

## 残留

- 麦克风采集不在范围内（用户明确：只录系统声，不混音）
- 仍无 CI
