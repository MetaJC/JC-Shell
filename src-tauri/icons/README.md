# Tauri Icons 说明
本目录应存放以下尺寸图标，M1 阶段暂未生成，请用以下任一方式生成：

方式 1：准备一张 1024x1024 PNG 命名为 icon.png，放在本目录，执行：
```
cd jc-shell-community
npm run tauri icon ../src-tauri/icons/icon.png
```

方式 2：手动放置：
- 32x32.png
- 128x128.png
- 128x128@2x.png
- icon.icns  (macOS)
- icon.ico   (Windows)

无图标时 Tauri 构建会失败，开发模式 `tauri dev` 不受影响。
