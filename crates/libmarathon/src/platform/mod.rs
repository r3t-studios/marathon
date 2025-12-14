//! Platform abstraction layer
//!
//! This module provides platform-agnostic interfaces for OS/hardware interaction:
//! - **input**: Abstract input events (keyboard, mouse, touch, gestures)
//! - **desktop**: Concrete winit-based implementation for desktop platforms
//! - **ios**: Concrete UIKit-based implementation for iOS
//!
//! The `run_executor()` function is the main entry point and automatically
//! selects the correct platform-specific executor based on compilation target.

use std::path::PathBuf;

pub mod input;

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(not(target_os = "ios"))]
pub mod desktop;

// Re-export the appropriate executor based on target platform
#[cfg(target_os = "ios")]
pub use ios::run_executor;

#[cfg(not(target_os = "ios"))]
pub use desktop::run_executor;

/// Sanitize app name for safe filesystem usage
///
/// Removes whitespace and converts to lowercase.
/// Example: "My App" -> "myapp"
pub fn sanitize_app_name(app_name: &str) -> String {
    app_name.replace(char::is_whitespace, "").to_lowercase()
}

/// Get the database filename for an application
///
/// Returns sanitized app name with .db extension.
/// Example: "My App" -> "myapp.db"
pub fn get_database_filename(app_name: &str) -> String {
    format!("{}.db", sanitize_app_name(app_name))
}

/// Get the full database path for an application
///
/// Combines platform-appropriate directory with sanitized database filename.
/// Example on iOS: "/path/to/Documents/myapp.db"
/// Example on desktop: "./myapp.db"
pub fn get_database_path(app_name: &str) -> PathBuf {
    get_data_directory().join(get_database_filename(app_name))
}

/// Get the platform-appropriate data directory
///
/// - iOS: Returns the app's Documents directory (via dirs crate)
/// - Desktop: Returns the current directory
pub fn get_data_directory() -> PathBuf {
    #[cfg(target_os = "ios")]
    {
        // Use dirs crate for proper iOS document directory access
        if let Some(doc_dir) = dirs::document_dir() {
            doc_dir
        } else {
            tracing::warn!("Failed to get iOS document directory, using current directory");
            PathBuf::from(".")
        }
    }

    #[cfg(not(target_os = "ios"))]
    {
        PathBuf::from(".")
    }
}
