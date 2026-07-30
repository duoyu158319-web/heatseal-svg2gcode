# HeatSeal SVG2GCode v2 更新说明

发布日期：2026-07-30

v2 在保留原版 `sameer/svg2gcode` SVG 解析与轨迹预览逻辑的基础上，完善了热封工艺设置、外封矩形和设备工作台，并重新整理了网页交互。

## 主要更新

### 1. 四级 Settings 结构

- Level 1 — Dwell Time：统一控制外封矩形和用户 SVG 的悬停时间，默认 `120 s`。
- Level 2 — Imported SVG：设置用户 SVG 的温度和工作高度，默认分别为 `230` 和 `0.12 mm`。
- Level 3 — Outer Frame：设置外封矩形、设备型号、矩形尺寸、温度和高度。
- Level 4 — Advanced Settings：折叠显示 Feedrate、Origin、Tolerance、DPI、路径优化和圆弧插补等高级参数。

外封矩形的温度和高度初始跟随 Imported SVG：修改 Imported SVG 的数值会同步到 Outer Frame；单独修改 Outer Frame 不会反向修改 Imported SVG。

### 2. 设备工作台与外封矩形

- 新增设备型号：`A1 (256 × 256 mm)` 和 `A2L (330 × 320 mm)`。
- 新增类似 Rhino 的网格工作台，包含 10 mm 小网格、50 mm 主网格、中心轴和实时外封矩形预览。
- 外封矩形默认 `150 × 150 mm`，宽高可分别自定义。
- 宽高不能超过当前设备工作台尺寸；输入无效时显示错误并禁用保存。
- 每次切换设备型号，外封矩形宽高统一恢复为 `150 × 150 mm`；重复选择当前型号不会重置。
- A1 和 A2L 的 G-code 机械中心均固定为 `X127.970 Y127.970`。设备型号只控制尺寸上限和工作台预览。
- 外封矩形始终围绕固定机械中心对称生成，并在用户 SVG 轨迹之前输出一次。

### 3. SVG 自动居中

- 新增 `Auto-center imported SVG` 开关。
- 开启时，根据用户 SVG 实际轨迹包围盒计算平移量，使轨迹中心对齐固定机械中心 `X127.970 Y127.970`。
- 关闭时保留 SVG 经原转换参数处理后的坐标。
- 自动居中只做平移，不缩放、不改变比例，也不影响外封矩形。

### 4. 热封 G-code 输出

- 每条有效独立 Stroke 使用完整热封周期输出。
- 默认工作高度已修正为 `Z0.12`。
- 外封矩形和用户 SVG 可使用不同温度与工作高度，共用悬停时间。
- Feedrate、Origin、Tolerance、DPI、路径优化和可选 G2/G3 圆弧插补继续影响轨迹生成。
- 网页热封输出不写入原通用输出中的 `G21`、`G90`、SVG 层级注释及 Begin/End、Tool On/Off 自定义序列。

### 5. 文件输入与输出

- 仅拖入 SVG：多个 SVG 按拖入顺序依次拼接，下载为一个 G-code 文件，不再生成 ZIP。
- SVG + G-code 模板：先将 SVG 转换成热封 G-code，再查找模板中包含 `标记` 的整行并完全替换，最后下载新的 G-code 文件。
- 原始 SVG 预览和 Toolpath Preview 继续使用原版预览逻辑。

### 6. 设置保存与兼容性

- Settings JSON 包装格式升级到 version 3，包含热封设置、设备型号、外封矩形和自动居中状态。
- 旧版网页设置和原项目 Settings JSON 仍可导入；缺失字段自动补默认值。
- 从旧版导入的外封矩形若超过 A1 上限，会恢复为 `150 × 150 mm`。
- 所有页面设置文字已统一为英文；G-code 固定工艺注释和模板识别关键字 `标记` 为兼容既有工作流而保留中文。

## 验证结果

- Web Rust 单元测试：22 项通过。
- 核心 `svg2gcode` 测试：14 项通过。
- WebAssembly 编译检查通过。
- Trunk release 构建通过。
- 已在本地浏览器验证 A1/A2L 切换、尺寸限制、切换重置、同型号重复选择、实时矩形预览及保存禁用逻辑。

## 在线使用

<https://duoyu158319-web.github.io/heatseal-svg2gcode/>

## 代码来源

本项目基于 [sameer/svg2gcode](https://github.com/sameer/svg2gcode) 修改并继续遵循 MIT License。SVG 解析、曲线离散、路径排序和预览等基础能力来自原项目；v2 的热封模板、工作台、外封矩形、合并输出及模板替换功能由本项目扩展。
