use std::net::IpAddr;

#[cfg(target_os = "windows")]
use self::windows::WindowsKillSwitch;
#[cfg(target_os = "linux")]
use self::linux::LinuxKillSwitch;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

pub struct KillSwitch {
    #[cfg(target_os = "windows")]
    inner: WindowsKillSwitch,
    #[cfg(target_os = "linux")]
    inner: LinuxKillSwitch,
    enabled: bool,
}

impl KillSwitch {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            inner: WindowsKillSwitch::new(),
            #[cfg(target_os = "linux")]
            inner: LinuxKillSwitch::new(),
            enabled: false,
        }
    }

    pub async fn enable(&mut self, allowed_ips: Vec<IpAddr>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.enabled {
            // If already enabled, just update IPs if supported
            self.inner.update_allowed_ips(allowed_ips).await?;
            return Ok(());
        }
        self.inner.enable(allowed_ips).await?;
        self.enabled = true;
        Ok(())
    }

    pub async fn disable(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.enabled {
            return Ok(());
        }
        self.inner.disable().await?;
        self.enabled = false;
        Ok(())
    }
}
