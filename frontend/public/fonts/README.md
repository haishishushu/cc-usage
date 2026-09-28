# 原型字体

字体取自 Google Fonts 官方仓库，按各自 SIL Open Font License 随应用分发：

- Inter：<https://github.com/google/fonts/tree/main/ofl/inter>；许可证 `Inter-LICENSE.txt`。
- JetBrains Mono：<https://github.com/google/fonts/tree/main/ofl/jetbrainsmono>；许可证 `JetBrainsMono-LICENSE.txt`。

字体仅在本地加载，不向外部字体服务发送请求。中文沿用系统字体回退。

随应用分发的是 woff2 子集（拉丁、拉丁扩展 A、常用标点、上下标、货币、箭头、数学符号、制表符与几何图形），保留可变字重轴与全部 OpenType 特性。
由官方 `Inter-Variable.ttf` / `JetBrainsMono-Variable.ttf` 生成：

```
pyftsubset <字体>.ttf --flavor=woff2 --layout-features='*' \n  --unicodes="U+0000-017F,U+0192,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0300-0308,U+0394,U+03A9,U+03BC,U+03C0,U+2000-206F,U+2070-209F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2200-22FF,U+2500-257F,U+25A0-25FF,U+FEFF,U+FFFD"
```

界面新增子集外的符号时按上式追加区段重新生成。
