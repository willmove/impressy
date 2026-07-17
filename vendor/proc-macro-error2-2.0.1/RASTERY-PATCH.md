# Rastery patch for proc-macro-error2 2.0.1

This directory contains the crates.io source required through
`gpui 0.2.2 -> stacksafe 0.1.4 -> proc-macro-error2 2.0.1`.

The upstream crate re-exports `proc_macro` from a private `extern crate`
declaration. Rust future-incompatibility lint E0365 warns that this will become
a hard error (rust-lang/rust#127909). Rastery changes that one declaration to
`pub extern crate proc_macro`; no runtime or macro behavior is changed.

Upstream: <https://github.com/GnomedDev/proc-macro-error-2>

License: MIT OR Apache-2.0. The upstream license files are retained here.
Remove this patch when a synchronized GPUI/gpui-component upgrade no longer
pulls the affected release.
