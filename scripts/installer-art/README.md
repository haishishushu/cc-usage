# 安装向导画面资源

NSIS 安装向导的两张位图由这里的 HTML 源生成，产物写入
`backend/windows/installer/`，并由 `backend/tauri.conf.json` 的
`bundle.windows.nsis` 引用。

## 重新生成

```bash
pnpm installer-art        # 或 node scripts/installer-art/build.mjs
```

产物会被脚本自检（尺寸 + 位深），不通过直接报错退出。

## 文件

| 文件 | 用途 | 尺寸 |
|---|---|---|
| `sidebar.html` → `sidebar.bmp` | 欢迎页与完成页的左侧大图 | 164×314 |
| `header.html` → `header.bmp` | 目录页 / 安装页 / 完成页右上角标识 | 150×57 |
| `png-to-bmp.ps1` | PNG 转 24 位 BMP | — |
| `build.mjs` | 串起渲染与转换 | — |

## 几条不能改的约束

- **必须是 24 位 BMP。** MUI2 对 32 位带 alpha 的 BMP 会渲染成黑块或花屏，
  所以 `png-to-bmp.ps1` 强制 `Format24bppRgb`，`build.mjs` 会读 BMP 头复核。
- **尺寸写死。** 150×57 与 164×314 是 Tauri `NsisConfig` 规定的推荐值，
  改了会被 NSIS 拉伸变形。
- **`header.bmp` 底色必须是纯白。** MUI2 表头背景是纯白，换底色会露出色块边界。
- **`png-to-bmp.ps1` 必须带 UTF-8 BOM。** Windows PowerShell 5.1 对无 BOM 的
  文件按系统 ANSI 代码页解码，中文注释会直接变成语法错误。
- **渲染用 `chrome-headless-shell`，不要用完整版 `chrome.exe`。**
  后者加 `--headless=new` 截图在 Windows 上会挂住不退出，`build.mjs` 里
  已按这个优先级挑可执行文件，并加了 60 秒硬超时。

## 版本号

侧边图底部的版本号取自 `backend/tauri.conf.json` 的 `version`，
升版后重跑一次生成命令即可。左下角那行品牌字来自同一文件的
`bundle.copyright`，刻意不带版本号，避免两处不同步。
