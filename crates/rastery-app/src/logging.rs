//! Application logging setup and narrowly scoped dependency-log normalization.

use env_logger::Logger;
use log::{Log, Metadata, Record};

#[cfg(any(all(target_os = "windows", debug_assertions), test))]
const DXGI_SDK_COMPONENT_MISSING: &str = "0x887A002D";
#[cfg(any(all(target_os = "windows", debug_assertions), test))]
const GPUI_DIRECTX_DEVICES_SOURCE: &str = "gpui-0.2.2/src/platform/windows/directx_devices.rs";
#[cfg(any(all(target_os = "windows", debug_assertions), test))]
const GPUI_DXGI_PROBE_LINE: u32 = 99;
#[cfg(any(all(target_os = "windows", debug_assertions), test))]
const GPUI_DXGI_FOLLOW_UP: &str =
    "Failed to get DXGI debug interface. DirectX debugging features will be disabled.";

/// Installs the process logger while preserving `env_logger`'s `RUST_LOG` behavior.
pub(crate) fn init() {
    let logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
    let max_level = logger.filter();

    if log::set_boxed_logger(Box::new(RasteryLogger { inner: logger })).is_ok() {
        log::set_max_level(max_level);
    }
}

struct RasteryLogger {
    inner: Logger,
}

impl Log for RasteryLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        #[cfg(all(target_os = "windows", debug_assertions))]
        match classify_dxgi_debug_probe(
            record.level(),
            record.target(),
            record.file(),
            record.line(),
            &record.args().to_string(),
        ) {
            DxgiDebugProbeLog::ReplaceWithInfo => {
                self.inner.log(
                    &Record::builder()
                        .args(format_args!(
                            "Optional DirectX debug layer unavailable ({DXGI_SDK_COMPONENT_MISSING}); continuing without DirectX diagnostics."
                        ))
                        .level(log::Level::Info)
                        .target("rastery_app::startup")
                        .module_path(Some("rastery_app::logging"))
                        .build(),
                );
                return;
            }
            DxgiDebugProbeLog::SuppressDuplicate => return,
            DxgiDebugProbeLog::Forward => {}
        }

        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

#[cfg(any(all(target_os = "windows", debug_assertions), test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DxgiDebugProbeLog {
    Forward,
    ReplaceWithInfo,
    SuppressDuplicate,
}

#[cfg(any(all(target_os = "windows", debug_assertions), test))]
fn classify_dxgi_debug_probe(
    level: log::Level,
    target: &str,
    file: Option<&str>,
    line: Option<u32>,
    message: &str,
) -> DxgiDebugProbeLog {
    let source = file.unwrap_or_default().replace('\\', "/");
    if level == log::Level::Error
        && source.ends_with(GPUI_DIRECTX_DEVICES_SOURCE)
        && line == Some(GPUI_DXGI_PROBE_LINE)
        && message.contains(DXGI_SDK_COMPONENT_MISSING)
    {
        return DxgiDebugProbeLog::ReplaceWithInfo;
    }

    if level == log::Level::Warn
        && target == "gpui::platform::windows::directx_devices"
        && message == GPUI_DXGI_FOLLOW_UP
    {
        return DxgiDebugProbeLog::SuppressDuplicate;
    }

    DxgiDebugProbeLog::Forward
}

#[cfg(test)]
mod tests {
    use super::{
        DXGI_SDK_COMPONENT_MISSING, DxgiDebugProbeLog, GPUI_DXGI_FOLLOW_UP,
        classify_dxgi_debug_probe,
    };

    const SOURCE: &str = r"C:\registry\gpui-0.2.2\src\platform\windows\directx_devices.rs";

    #[test]
    fn expected_dxgi_probe_failure_is_replaced_with_info() {
        let action = classify_dxgi_debug_probe(
            log::Level::Error,
            "",
            Some(SOURCE),
            Some(99),
            &format!("Error {{ code: HRESULT({DXGI_SDK_COMPONENT_MISSING}) }}"),
        );

        assert_eq!(action, DxgiDebugProbeLog::ReplaceWithInfo);
    }

    #[test]
    fn gpui_follow_up_warning_is_suppressed_as_a_duplicate() {
        let action = classify_dxgi_debug_probe(
            log::Level::Warn,
            "gpui::platform::windows::directx_devices",
            Some(SOURCE),
            Some(115),
            GPUI_DXGI_FOLLOW_UP,
        );

        assert_eq!(action, DxgiDebugProbeLog::SuppressDuplicate);
    }

    #[test]
    fn other_directx_errors_are_forwarded() {
        let action = classify_dxgi_debug_probe(
            log::Level::Error,
            "",
            Some(SOURCE),
            Some(132),
            &format!("Error {{ code: HRESULT({DXGI_SDK_COMPONENT_MISSING}) }}"),
        );

        assert_eq!(action, DxgiDebugProbeLog::Forward);
    }
}
