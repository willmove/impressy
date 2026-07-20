# 启动 v2 并冻结 AI Provider 契约

## 状态

已接受（2026-07-19）。

## 背景

[ADR-0001](./0001-v1-scope-local-only.md) 将 AI 功能整体推迟到 v2；
[`design.md`](../spec/design.md) 的 Stage D 又要求在 v2 动工前重新研究当时有效的供应商接口。
v1 本地功能现已形成可自动验证的实现基线，因此本 ADR 启动 v2，但不改变任何本地功能的
离线、无 API Key、无遥测保证。

供应商接口按 2026-07-19 的一手文档核对：

- Seedream 使用 BytePlus ModelArk Image Generation API，默认模型
  `dola-seedream-5-0-pro-260628`，区域基址为
  `https://ark.ap-southeast.bytepluses.com/api/v3`。官方接口支持文本、参考图、
  `b64_json` 输出及 1–10 张参考图；[API reference](https://docs.byteplus.com/en/docs/ModelArk/1541523)。
- Google Nano Banana 使用 Gemini Interactions API，默认模型
  `gemini-3.1-flash-image`（Nano Banana 2），端点为
  `https://generativelanguage.googleapis.com/v1beta/interactions`；
  [image generation guide](https://ai.google.dev/gemini-api/docs/image-generation)。
- OpenAI 使用 Image API，默认模型 `gpt-image-2`，生成与编辑分别调用
  `https://api.openai.com/v1/images/generations` 与
  `https://api.openai.com/v1/images/edits`；
  [image generation guide](https://developers.openai.com/api/docs/guides/image-generation)。

这些模型名是应用默认值，不是永久协议。供应商适配器把模型名和线协议封装在 crate 内，
上层业务只依赖能力声明与统一请求/响应类型。

## 决策

### Workspace

v2 workspace 包含四个 crate：

1. `rastery-core`：继续只承载确定性的本地图像处理；
2. `rastery-ai`：Provider、能力声明、HTTPS 传输、错误归一化和系统凭据管理；
3. `rastery-presets`：编译进二进制且不向 UI 暴露的锁定提示词；
4. `rastery-app`：GPUI 页面、后台调度、设置、历史和结果下载。

### Provider 契约

`Provider` 是同步、`Send + Sync` 的接口。网络请求由 `rastery-app` 放入 GPUI background
executor；`rastery-ai` 不创建第二套异步运行时。统一契约包含：

- Provider 标识与能力声明；
- 文生图、参考图生图和可选区域编辑；
- 输出比例、生成数量和质量；
- 统一的图片字节响应；
- 无效凭据、余额不足、限流、网络失败、内容拦截、无效请求与供应商故障等语义错误。

每个适配器必须先按能力声明校验请求。所有生产端点必须是 HTTPS，TLS 证书由 Rustls 和
WebPKI 根证书验证。HTTP 传输通过内部 seam 可替换，以便在不消耗额度的情况下验证请求
序列化与错误映射。

### 凭据与配置

API Key 只进入操作系统凭据管理器：Windows Credential Manager、macOS Keychain、Linux
Secret Service。TOML 只保存默认 Provider、非敏感生成历史和本地输出路径。密钥类型不实现
明文 `Debug`/`Display`，日志不得记录请求头、请求体或凭据错误的原始值。

### 锁定提示词与档位

`rastery-presets/templates/` 中每项 AI 改图场景和行业工具各有独立模板文件，通过
`include_str!` 编译进二进制。UI 只传递工具与**档位**标识，绝不显示最终锁定提示词。

### 兼容边界

- Agnes 仍暂缓，只由 Provider trait 的扩展性覆盖；
- 三个视频入口继续为“开发中”，它们不属于标记为 v1/v2 的图片功能实现；
- AI 初始化、网络不可用或未配置 API Key 均不得阻止任何本地功能；
- v2 代码变更会使原 v1 桌面候选证据过期，发布前必须重新执行完整桌面验收。

## 后果

- `rastery-ai` 会引入 Rustls HTTPS 与系统凭据后端，需重新核对 30 MiB 发布体积门禁；
- 真实供应商结果、**保持不变项**、签名/公证与四种桌面环境仍需真人、真实 API Key 和对应
  平台完成验收；自动测试只能证明线协议、状态机、格式与安全不变量；
- 供应商更换模型或废弃端点时，只更新对应适配器及本 ADR 的后继决策，不让行业工具页面
  直接依赖供应商 JSON。
