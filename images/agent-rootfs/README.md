# Firecracker agent rootfs

该目录用于 Task 19（T-A7.1）产出 Firecracker rootfs 构建工件与 manifest。

## 目录约定

- `VERSION`：rootfs 基础版本号（默认 tag 来源）
- `rootfs-manifest.json`：最近一次构建生成的 manifest（由 `build-rootfs.sh` 写入）
- `out/<tag>/rootfs/`：解包后的 rootfs 目录
- `out/<tag>/rootfs.tar.gz`：可分发 rootfs 压缩包
- `out/<tag>/rootfs.tar.gz.sha256`：压缩包校验和

## 使用方式

```bash
bash scripts/firecracker/build-rootfs.sh
bash scripts/firecracker/verify-rootfs.sh
```

如需可重复构建，可固定 `SOURCE_DATE_EPOCH`：

```bash
SOURCE_DATE_EPOCH=1700000000 bash scripts/firecracker/build-rootfs.sh --tag v0.1.0
```

## 验收映射（Task 19）

- 构建成功：`bash scripts/firecracker/build-rootfs.sh` 退出码 0
- 内容校验：`bash scripts/firecracker/verify-rootfs.sh` 报告 python + agent-lite 可用
- 证据文件：
  - `.sisyphus/evidence/task-19-rootfs-build.txt`
  - `.sisyphus/evidence/task-19-rootfs-missing-runtime.txt`
