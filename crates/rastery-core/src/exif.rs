//! EXIF 元数据查看与清除。
//!
//! 覆盖 FR-06、Requirement 11、55.5、56.1、NFR-06。
//!
//! # 为什么在容器层剥离而不是重新编码
//!
//! Requirement 55.5 要求「清除 EXIF 后输出像素与输入完全一致」。若走
//! 解码 → 重新编码的路子，JPEG 的有损重编码必然改变像素，该要求直接不成立。
//! 因此本模块用 `img-parts` 在**容器层**操作：只删除元数据段，压缩后的图像
//! 数据原样搬运，像素分毫不动。

use crate::error::{CoreError, Result};
use img_parts::{Bytes, DynImage, ImageEXIF};

/// 从 EXIF 读出的拍摄参数（Requirement 11.1–11.6）。
///
/// 各字段均为 `Option`——不是每张照片都带全部信息，缺失是正常情况而非错误。
/// 值以人类可读的字符串给出（如光圈 `f/2.8`、快门 `1/250`），因为 UI 要直接展示，
/// 而 EXIF 的原始有理数表示对用户无意义。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExifData {
    /// 相机厂商（Requirement 11.1）。属设备识别信息。
    pub camera_make: Option<String>,
    /// 相机型号（Requirement 11.1）。属设备识别信息。
    pub camera_model: Option<String>,
    /// 镜头型号。属设备识别信息。
    pub lens_model: Option<String>,
    /// 焦距（Requirement 11.2）。
    pub focal_length: Option<String>,
    /// 光圈（Requirement 11.3）。
    pub aperture: Option<String>,
    /// 快门速度（Requirement 11.4）。
    pub shutter_speed: Option<String>,
    /// ISO 感光度（Requirement 11.5）。
    pub iso: Option<String>,
    /// 拍摄时间。
    pub taken_at: Option<String>,
    /// GPS 坐标（Requirement 11.6）。属隐私信息。
    pub gps: Option<GpsCoordinates>,
}

impl ExifData {
    /// 是否含有需要清除的隐私信息：GPS 或设备识别字段。
    ///
    /// 对应 NFR-06 与 Requirement 11.7、11.8 关心的两类字段。
    pub fn has_privacy_data(&self) -> bool {
        self.gps.is_some()
            || self.camera_make.is_some()
            || self.camera_model.is_some()
            || self.lens_model.is_some()
    }
}

/// GPS 坐标（Requirement 11.6）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsCoordinates {
    /// 纬度，正数为北纬。
    pub latitude: f64,
    /// 经度，正数为东经。
    pub longitude: f64,
}

impl Eq for GpsCoordinates {}

/// 读取图像的 EXIF（Requirement 11.1–11.6）。
///
/// 返回 `Ok(None)` 表示图像不含 EXIF——这是正常情况（如截图、PNG），不是错误。
/// 只有当数据根本不是可解析的图像时才返回 `Err`。
pub fn read(bytes: &[u8]) -> Result<Option<ExifData>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let reader = exif::Reader::new();
    let exif_data = match reader.read_from_container(&mut cursor) {
        Ok(e) => e,
        // 没有 EXIF 段 / 格式不带 EXIF：不是错误
        Err(exif::Error::NotFound(_)) | Err(exif::Error::BlankValue(_)) => return Ok(None),
        Err(e) => {
            return Err(CoreError::Exif {
                detail: e.to_string(),
            });
        }
    };

    let get = |tag: exif::Tag| -> Option<String> {
        exif_data
            .get_field(tag, exif::In::PRIMARY)
            .map(|f| f.display_value().with_unit(&exif_data).to_string())
    };

    Ok(Some(ExifData {
        camera_make: get(exif::Tag::Make),
        camera_model: get(exif::Tag::Model),
        lens_model: get(exif::Tag::LensModel),
        focal_length: get(exif::Tag::FocalLength),
        aperture: get(exif::Tag::FNumber),
        shutter_speed: get(exif::Tag::ExposureTime),
        iso: get(exif::Tag::PhotographicSensitivity),
        taken_at: get(exif::Tag::DateTimeOriginal),
        gps: read_gps(&exif_data),
    }))
}

fn read_gps(e: &exif::Exif) -> Option<GpsCoordinates> {
    let lat = read_coordinate(e, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef, b'S')?;
    let lon = read_coordinate(e, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef, b'W')?;
    Some(GpsCoordinates {
        latitude: lat,
        longitude: lon,
    })
}

/// 把 EXIF 的「度/分/秒」三元有理数转成十进制度，并按参考方向定正负。
fn read_coordinate(
    e: &exif::Exif,
    value_tag: exif::Tag,
    ref_tag: exif::Tag,
    negative_ref: u8,
) -> Option<f64> {
    let field = e.get_field(value_tag, exif::In::PRIMARY)?;
    let dms = match &field.value {
        exif::Value::Rational(v) if v.len() >= 3 => v,
        _ => return None,
    };
    let degrees = dms[0].to_f64() + dms[1].to_f64() / 60.0 + dms[2].to_f64() / 3600.0;

    let is_negative = e
        .get_field(ref_tag, exif::In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Ascii(v) => v.first().and_then(|s| s.first()).copied(),
            _ => None,
        })
        .is_some_and(|c| c.eq_ignore_ascii_case(&negative_ref));

    Some(if is_negative { -degrees } else { degrees })
}

/// 清除图像的全部 EXIF 元数据（Requirement 11.7、11.8、NFR-06）。
///
/// 删除整个 EXIF 段，因此 GPS 与设备识别字段一并消失——这比逐字段删除更可靠，
/// 后者容易漏掉 MakerNote 之类的厂商私有字段。
///
/// **像素不变**（Requirement 55.5）：只动容器的元数据段，压缩数据原样保留。
///
/// **幂等**（Requirement 56.1）：图像本就不含 EXIF 时**原样返回输入字节**，
/// 不做任何重写。这让幂等性是结构上保证的，而不是碰巧成立。
///
/// ICC 色彩配置**予以保留**——它不是隐私数据，删掉会让图像颜色渲染走样。
///
/// 输入不是 JPEG/PNG/WebP 时返回 [`CoreError::Exif`]——其余格式本引擎不支持
/// 容器层元数据操作。
pub fn strip(bytes: &[u8]) -> Result<Vec<u8>> {
    let buf = Bytes::copy_from_slice(bytes);
    let mut image = DynImage::from_bytes(buf)
        .map_err(|e| CoreError::Exif {
            detail: e.to_string(),
        })?
        .ok_or_else(|| CoreError::Exif {
            detail: "不是 JPEG / PNG / WebP，无法在容器层清除元数据".to_string(),
        })?;

    // 已经没有 EXIF：原样返回，保证幂等（Requirement 56.1）
    if image.exif().is_none() {
        return Ok(bytes.to_vec());
    }

    image.set_exif(None);
    Ok(image.encoder().bytes().to_vec())
}

/// 图像是否含有 EXIF 段。
///
/// 比 [`read`] 便宜，用于 UI 决定是否显示「清除」按钮。
pub fn has_exif(bytes: &[u8]) -> Result<bool> {
    let buf = Bytes::copy_from_slice(bytes);
    match DynImage::from_bytes(buf).map_err(|e| CoreError::Exif {
        detail: e.to_string(),
    })? {
        Some(img) => Ok(img.exif().is_some()),
        None => Ok(false),
    }
}
