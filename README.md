# zh2en-input · 中文输入，英文上屏

按一个快捷键，弹出一个类似 Spotlight 的小输入框。用你平时的中文输入法打字，下方实时显示英文预览，**回车后英文直接粘贴到你刚才所在的窗口**。

适合用英文写 Slack、GitHub、邮件，但想用中文思考和打字的场景。

```
任意应用里  ──Ctrl/⌘+Shift+Space──▶  ┌────────────────────────────┐
                                    │ 明天下午在飞书上过一下方案   │  ← 用你自己的中文输入法
                                    │ ┌────────────────────────┐ │
                                    │ │ Let's go over the plan │ │  ← 实时英文预览
                                    │ │ on Lark tomorrow       │ │
                                    │ │ afternoon.             │ │
                                    │ └────────────────────────┘ │
                                    └────────────────────────────┘
                     Enter ──▶ 弹窗消失，焦点回到原窗口，英文被粘贴进去
```

## 特性

- **不替换输入法**：继续用搜狗、微信、Rime、系统自带输入法都行。选词时按 Enter 不会误触发上屏（已处理 IME composition 状态）。
- **先预览再上屏**：停止打字约 350 ms 后自动翻译；按 Enter 时如果预览已就绪，直接使用，不再重复请求。
- **翻译服务可替换**：
  - 任何 OpenAI 兼容接口：DeepSeek（默认）、通义千问、Kimi、OpenAI，以及本地 Ollama
  - DeepL
  - `mock`：离线调试用
- **术语表**：固定译法，例如“飞书 → Lark”。
- **LRU 缓存**：同一句话第二次翻译时直接命中缓存，0 ms。
- **剪贴板保护**：粘贴后自动恢复你原来的剪贴板文本。
- **两种上屏方式**：`paste`（剪贴板 + Ctrl/⌘+V，默认）和 `type`（模拟逐字键入，用于禁止粘贴的应用）。
- **跨平台**：Windows / macOS / Linux (X11)。基于 Tauri 2，安装包只有几 MB。

## 快速开始

### 1. 安装依赖

- [Rust](https://rustup.rs/)（stable）
- Node.js 20+
- 各平台的 Tauri 依赖：<https://v2.tauri.app/start/prerequisites/>
  - Windows：WebView2（Win10/11 已自带）+ MSVC Build Tools
  - macOS：Xcode Command Line Tools
  - Ubuntu：`sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`

### 2. 运行

```bash
npm install
export DEEPSEEK_API_KEY=sk-...    # 或者稍后写进配置文件
npm run tauri dev
```

首次启动时会弹出输入框，系统托盘里会出现图标。之后在任何地方按 **Ctrl+Shift+Space**（macOS 为 **⌘+⇧+Space**）即可唤起。

### 3. 打包

```bash
npm run tauri build
```

也可以推送 `v*` 格式的 tag（例如 `git tag v0.1.0 && git push --tags`），GitHub Actions 会自动构建三个平台的安装包，并附加到一个 Draft Release 上。

## 快捷键

| 按键 | 作用 |
|---|---|
| `Ctrl+Shift+Space` / `⌘+⇧+Space` | 打开 / 关闭输入框（可在配置中修改，例如 `"Alt+Space"`） |
| `Enter` | 翻译并粘贴到原窗口 |
| `Ctrl+Enter` / `⌘+Enter` | 只复制英文到剪贴板，不粘贴 |
| `Shift+Enter` | 换行 |
| `Esc` / 点击别处 | 取消 |

## 配置

配置文件在首次启动时自动生成，可通过托盘菜单「编辑配置文件…」打开。保存后，下次打开输入框时自动生效，无需重启。

| 系统 | 路径 |
|---|---|
| Windows | `%APPDATA%\ai.lanco.zh2en-input\config.json` |
| macOS | `~/Library/Application Support/ai.lanco.zh2en-input/config.json` |
| Linux | `~/.config/ai.lanco.zh2en-input/config.json` |

完整示例见 [`config.example.json`](config.example.json)。常用的翻译服务配置如下：

```jsonc
// DeepSeek（默认）
"provider": { "kind": "openai", "base_url": "https://api.deepseek.com/v1", "model": "deepseek-chat", "api_key_env": "DEEPSEEK_API_KEY" }

// 通义千问（DashScope 兼容模式）
"provider": { "kind": "openai", "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1", "model": "qwen-plus", "api_key_env": "DASHSCOPE_API_KEY" }

// 本地 Ollama（无需 key，离线可用）
"provider": { "kind": "openai", "base_url": "http://localhost:11434/v1", "model": "qwen2.5:7b" }

// DeepL（Free 版 key 以 :fx 结尾，会自动使用 api-free 域名）
"provider": { "kind": "deepl", "api_key_env": "DEEPL_API_KEY", "target_lang": "EN-US" }
```

`style` 字段可以指定语气，例如 `"casual Slack chat"` 或 `"formal business email"`。

> 模型名称会随服务商更新而变化，请以服务商文档为准。

## 平台说明

- **macOS**：第一次上屏时，系统会请求「辅助功能」权限（系统设置 › 隐私与安全性 › 辅助功能），用于模拟 ⌘V。授权后可能需要重启应用。
- **Windows**：如果目标程序以管理员身份运行，而本工具没有，Windows 会拦截模拟按键（UIPI）。遇到这种情况，请以管理员身份运行本工具。
- **Linux**：支持 X11。Wayland 会限制全局快捷键和模拟输入，可以在 XWayland 下使用，或通过桌面环境的自定义快捷键再次启动本程序（第二次启动会唤起已运行实例的输入框）。
- 有些场景会拦截粘贴，例如密码框、部分游戏、远程桌面。可以把 `commit_method` 改为 `"type"`，或者用 `Ctrl+Enter` 只复制、再手动粘贴。

## 项目结构

```
crates/zh2en-core/      翻译核心（与 UI 无关）：Translator trait、各服务实现、缓存、术语表、配置
  src/providers/        openai.rs · deepl.rs · mock.rs
src-tauri/              桌面应用
  src/lib.rs            托盘、全局快捷键、弹窗显示/隐藏、Tauri 命令、配置热加载
  src/commit.rs         上屏线程：剪贴板保存 → 写入 → 模拟粘贴 → 恢复
  src/focus.rs          记录与恢复目标窗口焦点（Windows 用 HWND；macOS 通过隐藏应用归还焦点）
src/                    弹窗前端（TypeScript，无框架）
```

翻译核心被设计成可以独立复用。后续接入 Rime（librime-lua）插件或本地常驻服务时，可以直接复用 `zh2en-core`。

## 开发

```bash
cargo test -p zh2en-core     # 核心单元测试（含基于 wiremock 的 HTTP 测试）
npm run build                # 前端类型检查与构建
cargo build -p zh2en-input   # 编译桌面应用
```

调试界面时，把配置改成 `"provider": { "kind": "mock" }`，不需要联网，也不需要 API key。

## 路线图

- [ ] 流式预览：边生成边显示
- [ ] 设置界面（替代手动编辑 JSON）
- [ ] 历史记录与收藏
- [ ] 本地翻译模型（CTranslate2 / ONNX，离线且低延迟）
- [ ] 将 `zh2en-core` 暴露为本地 HTTP 服务，提供 Rime 插件

## License

MIT
