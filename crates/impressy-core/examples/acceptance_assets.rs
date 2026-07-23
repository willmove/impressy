//! Generate deterministic assets for the v1 desktop acceptance checklist.

use std::io::Read;
use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use impressy_core::format::{self, EncodeSettings, PngCompression};
use impressy_core::qr::{self, QrOptions};
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/acceptance-assets"));
    std::fs::create_dir_all(&output)?;

    write_png(&output.join("large-50mp.png"), &pattern(8_000, 6_250, 17))?;
    write_png(
        &output.join("screenshot-1920x1080.png"),
        &pattern(1_920, 1_080, 29),
    )?;
    write_png(
        &output.join("transparent-1024.png"),
        &transparent_pattern(1_024, 1_024),
    )?;

    let batch = output.join("batch-100");
    std::fs::create_dir_all(&batch)?;
    for index in 0..100u32 {
        write_png(
            &batch.join(format!("batch-{index:03}.png")),
            &pattern(256, 256, index as u8),
        )?;
    }

    let qr = qr::generate(
        "https://example.com/impressy-acceptance",
        QrOptions::default(),
    )?;
    write_png(&output.join("qr-screenshot-quality.png"), &qr)?;

    let digest = directory_digest(&output)?;
    println!(
        "generated deterministic acceptance assets in {}",
        output.display()
    );
    println!("canonical resource SHA-256: {digest}");
    Ok(())
}

fn directory_digest(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    for (relative, path) in files {
        digest.update(relative.as_bytes());
        digest.update([0]);
        let mut file = std::fs::File::open(path)?;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)?
                .iter()
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((relative, path));
        }
    }
    Ok(())
}

fn write_png(path: &Path, image: &RgbaImage) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = format::encode(
        image,
        EncodeSettings::Png {
            compression: PngCompression::Fast,
        },
    )?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn pattern(width: u32, height: u32, seed: u8) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let block = ((x / 32) ^ (y / 32)) as u8;
        Rgba([
            x.wrapping_mul(13) as u8 ^ seed,
            y.wrapping_mul(7) as u8 ^ seed.rotate_left(1),
            block.wrapping_add(seed),
            255,
        ])
    })
}

fn transparent_pattern(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        let alpha = ((x + y) % 256) as u8;
        Rgba([40, x as u8, y as u8, alpha])
    })
}
