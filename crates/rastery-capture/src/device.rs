//! 屏幕捕获设备抽象（FR-08 截图、FR-09 取色）。
//!
//! 本模块定义 [`CaptureDevice`] trait 与相关几何类型，对应 design.md §Capture Module。
//!
//! **环境约束（ADR-0002 / CLAUDE.md）**：主开发机是无头 VM——无 `DISPLAY`、0 显示器、
//! 模拟显卡。真正的截图 / 取色 / 热键实现是**平台特定**且**只能在真机验收**的，本机
//! 连 `#[cfg(windows)]` 分支都不参与编译。因此本 crate 默认只提供 [`NullCaptureDevice`]
//! （无头回退，一律返回 [`CaptureError::Unsupported`]），真实平台实现在真机分支上补齐。

use crate::error::{CaptureError, Result};
use async_trait::async_trait;
use image::{Rgba, RgbaImage};

/// 显示器标识。多屏环境下每块屏一个稳定 id。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayId(pub u32);

/// 显示器信息（design.md `DisplayInfo`）。
///
/// 同时保留物理分辨率与逻辑分辨率，因为混合 DPI 环境下二者不一致，
/// 截图精度（Property 16：捕获像素尺寸须按 DPI 缩放严格匹配视觉选区）依赖它们。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayInfo {
    /// 显示器 id。
    pub id: DisplayId,
    /// 物理分辨率（像素）。
    pub physical_resolution: (u32, u32),
    /// 逻辑分辨率（DIP）。
    pub logical_resolution: (u32, u32),
    /// DPI 缩放比（1.0 / 1.25 / 1.5 / 2.0）。
    pub dpi_scale: f32,
    /// 在全局虚拟桌面中的左上角位置。
    pub position: (i32, i32),
}

/// 物理像素坐标系下的矩形选区（design.md `PhysicalRegion`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRegion {
    /// 左上角 x（物理像素）。
    pub x: u32,
    /// 左上角 y（物理像素）。
    pub y: u32,
    /// 宽（物理像素）。
    pub width: u32,
    /// 高（物理像素）。
    pub height: u32,
}

impl PhysicalRegion {
    /// 校验非空。宽或高为 0 时返回错误。
    pub fn checked(x: u32, y: u32, width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(CaptureError::InvalidArgument(format!(
                "选区宽高必须为正，收到 {width}×{height}"
            )));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
}

/// 物理像素坐标系下的点（design.md `PhysicalPoint`）。取色用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPoint {
    /// x（物理像素）。
    pub x: u32,
    /// y（物理像素）。
    pub y: u32,
}

/// 修饰键（与 `rastery_core::config::Modifier` 语义一致，此处不引入 core 依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// Ctrl。
    Ctrl,
    /// Alt / Option。
    Alt,
    /// Shift。
    Shift,
    /// Super / Cmd / Win。
    Super,
}

/// 全局热键（FR-08，Requirement 32.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    /// 修饰键集合。
    pub modifiers: Vec<Modifier>,
    /// 主键，大写存储，如 `"X"`。
    pub key: String,
}

/// 屏幕捕获设备（design.md `CaptureDevice`）。
///
/// 全部方法都是**平台特定**的，无法在无头 VM 上验证。用 `async_trait` 以保持
/// `Box<dyn CaptureDevice>` 的对象安全（design.md `AppState::capture_device`）。
#[async_trait]
pub trait CaptureDevice: Send + Sync {
    /// 注册全局截图热键（FR-08）。
    fn register_hotkey(&self, hotkey: Hotkey) -> Result<()>;

    /// 注销已注册的热键。
    fn unregister_hotkey(&self) -> Result<()>;

    /// 枚举当前所有显示器（含 DPI）。
    fn detect_displays(&self) -> Vec<DisplayInfo>;

    /// 捕获指定显示器上的矩形区域（FR-08）。
    async fn capture_region(&self, display: DisplayId, region: PhysicalRegion) -> Result<RgbaImage>;

    /// 采样指定显示器上某点的颜色（FR-09 取色）。
    async fn sample_pixel(&self, display: DisplayId, position: PhysicalPoint) -> Result<Rgba<u8>>;
}

/// 无头环境的空实现：一切返回 [`CaptureError::Unsupported`]，无显示器。
///
/// 用途：让 `rastery-capture` 在无头 VM 上照常编译、`AppState` 能构造，且系统集成
/// 尚未在某平台落地时有一个不 panic 的安全占位。真机分支用真实设备替换它。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullCaptureDevice;

#[async_trait]
impl CaptureDevice for NullCaptureDevice {
    fn register_hotkey(&self, _hotkey: Hotkey) -> Result<()> {
        Err(CaptureError::Unsupported("全局热键注册"))
    }

    fn unregister_hotkey(&self) -> Result<()> {
        Err(CaptureError::Unsupported("全局热键注销"))
    }

    fn detect_displays(&self) -> Vec<DisplayInfo> {
        Vec::new()
    }

    async fn capture_region(
        &self,
        _display: DisplayId,
        _region: PhysicalRegion,
    ) -> Result<RgbaImage> {
        Err(CaptureError::Unsupported("屏幕捕获"))
    }

    async fn sample_pixel(
        &self,
        _display: DisplayId,
        _position: PhysicalPoint,
    ) -> Result<Rgba<u8>> {
        Err(CaptureError::Unsupported("屏幕取色"))
    }
}

// —— 真机平台后端（`system` feature）——
//
// 用 `xcap` 做跨平台截图 / 取色 / 多屏枚举，`global-hotkey` 做全局热键。
// **本机（无头 VM）不编译此段**：xcap 的 Linux 后端需要 wayland / pipewire / xcb 等
// 系统库，且无显示服务器时行为无从验证。真机以 `--features system` 构建（ADR-0002）。
#[cfg(feature = "system")]
mod system {
    use super::{
        CaptureDevice, DisplayId, DisplayInfo, Hotkey, Modifier, PhysicalPoint, PhysicalRegion,
    };
    use crate::error::{CaptureError, Result};
    use async_trait::async_trait;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::GlobalHotKeyManager;
    use image::{Rgba, RgbaImage};
    use std::str::FromStr;
    use std::sync::Mutex;
    use xcap::Monitor;

    /// 基于 `xcap` + `global-hotkey` 的真实系统集成设备。
    pub struct SystemCaptureDevice {
        hotkey_manager: GlobalHotKeyManager,
        /// 当前已注册的热键，用于注销。
        registered: Mutex<Option<HotKey>>,
    }

    impl SystemCaptureDevice {
        /// 新建设备。热键管理器须在主线程创建（平台要求）。
        pub fn new() -> Result<Self> {
            let hotkey_manager = GlobalHotKeyManager::new()
                .map_err(|e| CaptureError::Platform(format!("热键管理器初始化失败：{e}")))?;
            Ok(Self {
                hotkey_manager,
                registered: Mutex::new(None),
            })
        }

        /// 找到指定 id 的显示器。
        fn monitor(display: DisplayId) -> Result<Monitor> {
            let monitors = Monitor::all()
                .map_err(|e| CaptureError::Platform(format!("枚举显示器失败：{e}")))?;
            for m in monitors {
                if m.id().map(|id| id == display.0).unwrap_or(false) {
                    return Ok(m);
                }
            }
            Err(CaptureError::DisplayNotFound(display.0))
        }
    }

    /// 把领域内的 [`Hotkey`] 转成 `global-hotkey` 的 [`HotKey`]。
    fn to_global_hotkey(hotkey: &Hotkey) -> Result<HotKey> {
        let mut mods = Modifiers::empty();
        for m in &hotkey.modifiers {
            mods |= match m {
                Modifier::Ctrl => Modifiers::CONTROL,
                Modifier::Alt => Modifiers::ALT,
                Modifier::Shift => Modifiers::SHIFT,
                Modifier::Super => Modifiers::SUPER,
            };
        }
        // 单个字母键按 `KeyX` 解析；其余（如 `PrintScreen`）直接按 code 名解析。
        let key = hotkey.key.trim();
        let code_str = if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) {
            format!("Key{}", key.to_ascii_uppercase())
        } else {
            key.to_string()
        };
        let code = Code::from_str(&code_str)
            .map_err(|_| CaptureError::InvalidArgument(format!("无法识别的按键：{key}")))?;
        Ok(HotKey::new(Some(mods), code))
    }

    #[async_trait]
    impl CaptureDevice for SystemCaptureDevice {
        fn register_hotkey(&self, hotkey: Hotkey) -> Result<()> {
            let gh = to_global_hotkey(&hotkey)?;
            self.hotkey_manager
                .register(gh)
                .map_err(|e| CaptureError::Platform(format!("注册热键失败：{e}")))?;
            *self.registered.lock().expect("registered 锁") = Some(gh);
            Ok(())
        }

        fn unregister_hotkey(&self) -> Result<()> {
            let taken = self.registered.lock().expect("registered 锁").take();
            if let Some(gh) = taken {
                self.hotkey_manager
                    .unregister(gh)
                    .map_err(|e| CaptureError::Platform(format!("注销热键失败：{e}")))?;
            }
            Ok(())
        }

        fn detect_displays(&self) -> Vec<DisplayInfo> {
            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(_) => return Vec::new(),
            };
            monitors
                .into_iter()
                .filter_map(|m| {
                    let id = m.id().ok()?;
                    let scale = m.scale_factor().ok()?;
                    let (pw, ph) = (m.width().ok()?, m.height().ok()?);
                    // xcap 的 width/height 为物理像素；逻辑尺寸按缩放反推。
                    let logical = if scale > 0.0 {
                        (
                            (pw as f32 / scale).round() as u32,
                            (ph as f32 / scale).round() as u32,
                        )
                    } else {
                        (pw, ph)
                    };
                    Some(DisplayInfo {
                        id: DisplayId(id),
                        physical_resolution: (pw, ph),
                        logical_resolution: logical,
                        dpi_scale: scale,
                        position: (m.x().ok()?, m.y().ok()?),
                    })
                })
                .collect()
        }

        async fn capture_region(
            &self,
            display: DisplayId,
            region: PhysicalRegion,
        ) -> Result<RgbaImage> {
            let monitor = Self::monitor(display)?;
            monitor
                .capture_region(region.x, region.y, region.width, region.height)
                .map_err(|e| CaptureError::Platform(format!("截图失败：{e}")))
        }

        async fn sample_pixel(
            &self,
            display: DisplayId,
            position: PhysicalPoint,
        ) -> Result<Rgba<u8>> {
            // 取 1×1 区域即为该点颜色。
            let img = self
                .capture_region(
                    display,
                    PhysicalRegion {
                        x: position.x,
                        y: position.y,
                        width: 1,
                        height: 1,
                    },
                )
                .await?;
            img.get_pixel_checked(0, 0).copied().ok_or_else(|| {
                CaptureError::RegionOutOfBounds {
                    detail: format!("取色点 ({}, {}) 超出显示器", position.x, position.y),
                }
            })
        }
    }
}

#[cfg(feature = "system")]
pub use system::SystemCaptureDevice;
