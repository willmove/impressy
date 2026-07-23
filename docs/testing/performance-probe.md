# Core 性能探针

`impressy-core` 提供一个无额外依赖的 release 性能探针，用于在相同机器上比较提交前后的
相对性能：

```shell
cargo bench -p impressy-core --bench v1_local
```

探针依次执行 12MP Lanczos3 缩放、1080p 截图美化，以及 100 张小图的缩放与 PNG 编码，
输出单次耗时和批处理吞吐量。它使用确定性生成图，不依赖仓库外的测试资源。

该探针不是跨机器稳定的硬门禁，也不替代
[`v1-desktop-acceptance.md`](./v1-desktop-acceptance.md) 中的冷启动、50MP 预览、窗口响应和
进度刷新验收。正式对比时应在固定硬件上至少运行 3 次，记录最大值、Rust 版本、操作系统
和提交 SHA；发现明显回退后再用 profiler 定位。
