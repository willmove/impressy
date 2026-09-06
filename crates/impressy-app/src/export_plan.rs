//! Collision-safe publication of encoded image files.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CollisionPolicy {
    #[default]
    PreserveBoth,
    Skip,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublishOutcome {
    Written(PathBuf),
    Skipped(PathBuf),
}

pub(crate) fn publish_image(
    requested_path: &Path,
    bytes: &[u8],
    policy: CollisionPolicy,
) -> io::Result<PublishOutcome> {
    let Some(destination) = resolve_destination(requested_path, policy) else {
        return Ok(PublishOutcome::Skipped(requested_path.to_path_buf()));
    };
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp_path = temporary_path(&destination);
    let result = (|| {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        temp.write_all(bytes)?;
        temp.sync_all()?;
        drop(temp);

        let written = fs::read(&temp_path)?;
        image::load_from_memory(&written).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("temporary image validation failed: {error}"),
            )
        })?;

        publish_temp(&temp_path, &destination, policy)?;
        Ok(PublishOutcome::Written(destination))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

pub(crate) fn resolve_destination(
    requested_path: &Path,
    policy: CollisionPolicy,
) -> Option<PathBuf> {
    resolve_destinations(std::slice::from_ref(&requested_path.to_path_buf()), policy)
        .into_iter()
        .next()
        .flatten()
}

/// Resolve a complete output set before writing. Returned targets are unique within the task and
/// account for files already present on disk.
pub(crate) fn resolve_destinations(
    requested_paths: &[PathBuf],
    policy: CollisionPolicy,
) -> Vec<Option<PathBuf>> {
    let mut reserved = Vec::<PathBuf>::with_capacity(requested_paths.len());
    requested_paths
        .iter()
        .map(|requested_path| {
            let occupied = requested_path.exists()
                || reserved
                    .iter()
                    .any(|reserved_path| same_target(reserved_path, requested_path));
            let destination = match policy {
                CollisionPolicy::Skip if occupied => None,
                CollisionPolicy::Replace
                    if !reserved
                        .iter()
                        .any(|reserved_path| same_target(reserved_path, requested_path)) =>
                {
                    Some(requested_path.clone())
                }
                CollisionPolicy::Skip
                | CollisionPolicy::PreserveBoth
                | CollisionPolicy::Replace => {
                    if !occupied {
                        Some(requested_path.clone())
                    } else {
                        unique_destination(requested_path, &reserved)
                    }
                }
            };
            if let Some(destination) = &destination {
                reserved.push(destination.clone());
            }
            destination
        })
        .collect()
}

fn unique_destination(requested_path: &Path, reserved: &[PathBuf]) -> Option<PathBuf> {
    let parent = requested_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = requested_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("image");
    let extension = requested_path.extension().and_then(|value| value.to_str());
    for sequence in 2u64.. {
        let filename = match extension {
            Some(extension) => format!("{stem}-{sequence}.{extension}"),
            None => format!("{stem}-{sequence}"),
        };
        let candidate = parent.join(filename);
        if !candidate.exists()
            && !reserved
                .iter()
                .any(|reserved_path| same_target(reserved_path, &candidate))
        {
            return Some(candidate);
        }
    }
    unreachable!("the preserve-both sequence is unbounded")
}

fn same_target(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
    } else {
        left == right
    }
}

fn temporary_path(destination: &Path) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    destination.with_file_name(format!(
        ".{name}.impressy-tmp-{}-{sequence}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn publish_temp(temp: &Path, destination: &Path, policy: CollisionPolicy) -> io::Result<()> {
    if policy == CollisionPolicy::Replace {
        fs::rename(temp, destination)
    } else {
        publish_new_file(temp, destination)
    }
}

#[cfg(windows)]
fn publish_temp(temp: &Path, destination: &Path, policy: CollisionPolicy) -> io::Result<()> {
    if !destination.exists() {
        return publish_new_file(temp, destination);
    }
    if policy != CollisionPolicy::Replace {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination appeared while publishing",
        ));
    }
    debug_assert_eq!(policy, CollisionPolicy::Replace);
    atomic_replace_windows(destination, temp)
}

fn publish_new_file(temp: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(temp, destination)?;
    fs::remove_file(temp)
}

#[cfg(windows)]
fn atomic_replace_windows(destination: &Path, replacement: &Path) -> io::Result<()> {
    let destination = destination.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination path is not valid Unicode",
        )
    })?;
    let replacement = replacement.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "replacement path is not valid Unicode",
        )
    })?;
    winsafe::ReplaceFile(
        destination,
        replacement,
        None,
        winsafe::co::REPLACEFILE::WRITE_THROUGH,
    )
    .map_err(|error| io::Error::other(error.to_string()))
}

#[cfg(all(not(unix), not(windows)))]
fn publish_temp(temp: &Path, destination: &Path, policy: CollisionPolicy) -> io::Result<()> {
    if !destination.exists() {
        return publish_new_file(temp, destination);
    }
    if policy != CollisionPolicy::Replace {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination appeared while publishing",
        ));
    }
    debug_assert_eq!(policy, CollisionPolicy::Replace);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic replacement is unavailable on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use impressy_core::format::{self, EncodeSettings, PngCompression};
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::fs;

    fn png(value: u8) -> Vec<u8> {
        format::encode(
            &RgbaImage::from_pixel(2, 2, Rgba([value, 0, 0, 255])),
            EncodeSettings::Png {
                compression: PngCompression::Fast,
            },
        )
        .expect("encode fixture")
    }

    #[test]
    fn preserve_both_never_changes_existing_output() {
        let temp = tempfile::tempdir().expect("temp directory");
        let requested = temp.path().join("photo.png");
        fs::write(&requested, png(1)).expect("existing output");

        let outcome = publish_image(&requested, &png(2), CollisionPolicy::PreserveBoth)
            .expect("publish second output");

        assert_eq!(
            format::decode(&fs::read(&requested).unwrap())
                .unwrap()
                .get_pixel(0, 0)
                .0[0],
            1
        );
        let PublishOutcome::Written(second) = outcome else {
            panic!("preserve-both must write a second file");
        };
        assert_eq!(second.file_name().unwrap(), "photo-2.png");
        assert_eq!(
            format::decode(&fs::read(second).unwrap())
                .unwrap()
                .get_pixel(0, 0)
                .0[0],
            2
        );
    }

    #[test]
    fn invalid_replacement_leaves_existing_output_intact() {
        let temp = tempfile::tempdir().expect("temp directory");
        let requested = temp.path().join("photo.png");
        let original = png(1);
        fs::write(&requested, &original).expect("existing output");

        assert!(publish_image(&requested, b"not an image", CollisionPolicy::Replace).is_err());
        assert_eq!(fs::read(requested).unwrap(), original);
    }

    #[test]
    fn valid_replacement_atomically_publishes_the_new_image() {
        let temp = tempfile::tempdir().expect("temp directory");
        let requested = temp.path().join("photo.png");
        fs::write(&requested, png(1)).expect("existing output");

        publish_image(&requested, &png(2), CollisionPolicy::Replace)
            .expect("replace existing output");

        assert_eq!(
            format::decode(&fs::read(requested).unwrap())
                .unwrap()
                .get_pixel(0, 0)
                .0[0],
            2
        );
    }

    #[test]
    fn skip_reports_existing_path_without_writing() {
        let temp = tempfile::tempdir().expect("temp directory");
        let requested = temp.path().join("photo.png");
        let original = png(1);
        fs::write(&requested, &original).expect("existing output");

        assert_eq!(
            publish_image(&requested, &png(2), CollisionPolicy::Skip).unwrap(),
            PublishOutcome::Skipped(requested.clone())
        );
        assert_eq!(fs::read(requested).unwrap(), original);
    }

    proptest! {
        #[test]
        fn preserve_both_resolves_an_entire_unique_non_existing_target_set(
            names in proptest::collection::vec("[a-z]{1,5}", 1..24),
        ) {
            let temp = tempfile::tempdir().expect("temp directory");
            let requested = names
                .iter()
                .map(|name| temp.path().join(format!("{name}.png")))
                .collect::<Vec<_>>();
            for path in requested.iter().step_by(3) {
                fs::write(path, png(1)).expect("existing candidate");
            }

            let resolved = resolve_destinations(&requested, CollisionPolicy::PreserveBoth);
            let resolved = resolved
                .into_iter()
                .map(|path| path.expect("preserve-both always resolves a path"))
                .collect::<Vec<_>>();
            let unique = resolved.iter().cloned().collect::<HashSet<_>>();

            prop_assert_eq!(unique.len(), requested.len());
            prop_assert!(resolved.iter().all(|path| !path.exists()));
        }
    }
}
