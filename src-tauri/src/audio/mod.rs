pub mod capture;
pub mod encode;
pub mod mixer;

#[cfg(target_os = "macos")]
pub mod system;

#[cfg(target_os = "linux")]
pub mod system_linux;

#[cfg(target_os = "linux")]
pub use system_linux as system;
