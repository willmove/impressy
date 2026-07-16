//! 剪贴板集成（FR-08 截图复制、FR-09 色值复制）。
//!
//! 对应 design.md `ClipboardManager`。系统剪贴板访问是**平台特定**的，无头 VM 上
//! 无法验证，故此处定义 trait 与无头占位 [`NullClipboard`]；真实实现（GPUI 内置
//! 剪贴板优先，图像/特殊格式回退 `arboard`，见 spec §3.4）在真机分支补齐。

use crate::error::{CaptureError, Result};
use image::RgbaImage;

/// 系统剪贴板抽象（design.md `ClipboardManager`）。
pub trait Clipboard: Send + Sync {
    /// 把图像写入剪贴板（截图「复制到剪贴板」）。
    fn copy_image(&self, image: &RgbaImage) -> Result<()>;

    /// 把文本写入剪贴板（取色色值复制）。
    fn copy_text(&self, text: &str) -> Result<()>;

    /// 读取剪贴板中的图像；无图像时返回 `Ok(None)`。
    fn get_image(&self) -> Result<Option<RgbaImage>>;
}

/// 无头环境占位：写操作报 [`CaptureError::Unsupported`]，读操作返回空。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullClipboard;

impl Clipboard for NullClipboard {
    fn copy_image(&self, _image: &RgbaImage) -> Result<()> {
        Err(CaptureError::Unsupported("剪贴板写图像"))
    }

    fn copy_text(&self, _text: &str) -> Result<()> {
        Err(CaptureError::Unsupported("剪贴板写文本"))
    }

    fn get_image(&self) -> Result<Option<RgbaImage>> {
        Ok(None)
    }
}

// —— 真机平台后端（`system` feature）——
//
// 用 `arboard` 访问系统剪贴板。**本机（无头 VM）不编译此段**：无显示服务器时
// 剪贴板不可用且无从验证。真机以 `--features system` 构建（ADR-0002）。
#[cfg(feature = "system")]
mod system {
    use super::Clipboard;
    use crate::error::{CaptureError, Result};
    use arboard::{Clipboard as Arboard, Error as ArboardError, ImageData};
    use image::RgbaImage;
    use std::borrow::Cow;
    use std::sync::Mutex;

    /// 基于 `arboard` 的系统剪贴板。`arboard::Clipboard` 的方法需 `&mut self`，
    /// 故用 `Mutex` 提供内部可变性以匹配 [`Clipboard`] 的 `&self` 约定。
    pub struct SystemClipboard {
        inner: Mutex<Arboard>,
    }

    impl SystemClipboard {
        /// 新建剪贴板句柄。
        pub fn new() -> Result<Self> {
            let inner =
                Arboard::new().map_err(|e| CaptureError::Platform(format!("剪贴板初始化失败：{e}")))?;
            Ok(Self {
                inner: Mutex::new(inner),
            })
        }
    }

    impl Clipboard for SystemClipboard {
        fn copy_image(&self, image: &RgbaImage) -> Result<()> {
            let data = ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: Cow::Borrowed(image.as_raw()),
            };
            self.inner
                .lock()
                .expect("剪贴板锁")
                .set_image(data)
                .map_err(|e| CaptureError::Platform(format!("写入剪贴板图像失败：{e}")))
        }

        fn copy_text(&self, text: &str) -> Result<()> {
            self.inner
                .lock()
                .expect("剪贴板锁")
                .set_text(text)
                .map_err(|e| CaptureError::Platform(format!("写入剪贴板文本失败：{e}")))
        }

        fn get_image(&self) -> Result<Option<RgbaImage>> {
            let mut guard = self.inner.lock().expect("剪贴板锁");
            match guard.get_image() {
                Ok(img) => {
                    let (w, h) = (img.width as u32, img.height as u32);
                    let raw = img.bytes.into_owned();
                    RgbaImage::from_raw(w, h, raw)
                        .map(Some)
                        .ok_or_else(|| CaptureError::Platform("剪贴板图像缓冲区大小不符".into()))
                }
                // 剪贴板里没有图像不是错误。
                Err(ArboardError::ContentNotAvailable) => Ok(None),
                Err(e) => Err(CaptureError::Platform(format!("读取剪贴板图像失败：{e}"))),
            }
        }
    }
}

#[cfg(feature = "system")]
pub use system::SystemClipboard;
