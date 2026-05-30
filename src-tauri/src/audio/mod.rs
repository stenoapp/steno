pub mod capture;
pub mod encode;
pub mod meter;
pub mod mixer;
pub mod resample;

#[cfg(target_os = "macos")]
pub mod system;

#[cfg(target_os = "linux")]
pub mod system_linux;

#[cfg(target_os = "linux")]
pub use system_linux as system;

#[cfg(target_os = "windows")]
pub mod system_windows;

#[cfg(target_os = "windows")]
pub use system_windows as system;
