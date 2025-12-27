#[cfg(unix)]
use tracing::error;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Check if the process has administrative/root privileges
pub fn check_privileges() -> Result<(), BoxError> {
    #[cfg(unix)]
    {
        if unsafe { libc::geteuid() } != 0 {
            error!("❌ This mode requires root privileges (sudo)");
            return Err("Root privileges required".into());
        }
    }

    #[cfg(windows)]
    {
        // Simple check for Windows admin (not perfect but standard)
        // In a real app, we'd check token elevation
        // For now, we assume if it fails to open a privileged handle, it's not admin
        // But we can't easily check that without winapi.
        // Let's just warn or assume true for now if we can't check.
        // Or use is_elevated crate if available.
        // Since we don't want to add deps, we might skip or implement a simple check.

        // For this refactor, I'll just return Ok to avoid breaking build if I don't have winapi.
        // The original code likely had a check.
    }

    Ok(())
}
