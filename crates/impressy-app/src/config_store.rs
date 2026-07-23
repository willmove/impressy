//! v1 配置文件持久化。
//!
//! 配置位于 `directories` 给出的平台标准配置目录。解析失败时返回默认配置与警告，
//! 不让损坏文件阻止应用启动（Requirement 32、53.5）。

use std::fmt;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use impressy_core::config::{self, AppConfig};

/// 配置加载结果；`warning` 为 `Some` 表示已回退到默认配置。
#[derive(Debug)]
pub struct ConfigLoad {
    pub config: AppConfig,
    pub warning: Option<String>,
}

/// 配置文件存储位置与读写操作。
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// 使用平台标准配置目录创建存储器。
    pub fn for_current_user() -> Result<Self, ConfigStoreError> {
        let dirs = ProjectDirs::from("dev", "impressy", "Impressy")
            .ok_or(ConfigStoreError::ProjectDirectoryUnavailable)?;
        Ok(Self {
            path: dirs.config_dir().join("config.toml"),
        })
    }

    /// 仅供测试或显式嵌入场景指定配置路径。
    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 加载配置；文件不存在时静默使用默认值，损坏或不可读时携带警告回退。
    pub fn load(&self) -> ConfigLoad {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => match config::from_toml(&text) {
                Ok(config) => ConfigLoad {
                    config,
                    warning: None,
                },
                Err(error) => ConfigLoad {
                    config: AppConfig::default(),
                    warning: Some(error.to_string()),
                },
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ConfigLoad {
                config: AppConfig::default(),
                warning: None,
            },
            Err(error) => ConfigLoad {
                config: AppConfig::default(),
                warning: Some(error.to_string()),
            },
        }
    }

    /// 保存配置。先创建父目录，再写入临时文件并替换正式文件，避免半写入 TOML。
    pub fn save(&self, value: &AppConfig) -> Result<(), ConfigStoreError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| ConfigStoreError::InvalidPath(self.path.clone()))?;
        std::fs::create_dir_all(parent).map_err(|source| ConfigStoreError::Io {
            operation: "create config directory",
            path: parent.to_path_buf(),
            source,
        })?;

        let text = config::to_toml(value).map_err(|source| ConfigStoreError::Serialize {
            detail: source.to_string(),
        })?;
        let temporary = self.path.with_extension("toml.tmp");
        std::fs::write(&temporary, text).map_err(|source| ConfigStoreError::Io {
            operation: "write temporary config",
            path: temporary.clone(),
            source,
        })?;

        let backup = self.path.with_extension("toml.bak");
        let had_previous = self.path.exists();
        if had_previous {
            if backup.exists() {
                std::fs::remove_file(&backup).map_err(|source| ConfigStoreError::Io {
                    operation: "remove stale config backup",
                    path: backup.clone(),
                    source,
                })?;
            }
            std::fs::rename(&self.path, &backup).map_err(|source| ConfigStoreError::Io {
                operation: "back up config",
                path: self.path.clone(),
                source,
            })?;
        }

        match std::fs::rename(&temporary, &self.path) {
            Ok(()) => {
                if had_previous && backup.exists() {
                    let _ = std::fs::remove_file(backup);
                }
                Ok(())
            }
            Err(source) => {
                if had_previous && backup.exists() {
                    let _ = std::fs::rename(&backup, &self.path);
                }
                Err(ConfigStoreError::Io {
                    operation: "install new config",
                    path: self.path.clone(),
                    source,
                })
            }
        }
    }
}

#[derive(Debug)]
pub enum ConfigStoreError {
    ProjectDirectoryUnavailable,
    InvalidPath(PathBuf),
    Serialize {
        detail: String,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for ConfigStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectDirectoryUnavailable => {
                f.write_str("platform configuration directory is unavailable")
            }
            Self::InvalidPath(path) => write!(f, "invalid configuration path: {}", path.display()),
            Self::Serialize { detail } => write!(f, "configuration serialization failed: {detail}"),
            Self::Io {
                operation,
                path,
                source,
            } => write!(f, "{operation} failed for {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ConfigStoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use impressy_core::config::Language;

    #[test]
    fn save_load_round_trip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ConfigStore::at(temp.path().join("nested/config.toml"));
        let value = AppConfig {
            language: Language::En,
            last_output_dir: Some(temp.path().join("outputs")),
            ..AppConfig::default()
        };

        store.save(&value).expect("save config");
        let loaded = store.load();
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.config, value);
    }

    #[test]
    fn corrupted_config_falls_back_with_warning() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("config.toml");
        std::fs::write(&path, "not = [valid").expect("write corrupt config");
        let loaded = ConfigStore::at(path).load();

        assert_eq!(loaded.config, AppConfig::default());
        assert!(loaded.warning.is_some());
    }
}
