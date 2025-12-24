#[cfg(windows)]
pub mod service {
    use crate::Args;
    use clap::Parser;
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;
    use tracing::{error, info};
    use windows_service::{
        define_windows_service,
        service::{
            ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl,
            ServiceExitCode, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    const SERVICE_NAME: &str = "ZksVpnService";
    const SERVICE_DISPLAY_NAME: &str = "ZKS VPN Service";

    pub fn run() -> windows_service::Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }

    define_windows_service!(ffi_service_main, my_service_main);

    fn my_service_main(arguments: Vec<OsString>) {
        if let Err(e) = run_service_logic(arguments) {
            error!("Service failed: {:?}", e);
        }
    }

    fn run_service_logic(
        arguments: Vec<OsString>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let shutdown_tx_for_handler = shutdown_tx.clone();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop | ServiceControl::Interrogate => {
                    let _ = shutdown_tx_for_handler.send(());
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        // Parse arguments
        let mut args_vec: Vec<OsString> = vec!["zks-vpn".into()];
        args_vec.extend(arguments);

        let args = match Args::try_parse_from(args_vec) {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to parse service arguments: {}", e);
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    e.to_string(),
                )));
            }
        };

        let rt = tokio::runtime::Runtime::new()?;

        #[cfg(feature = "vpn")]
        let vpn_controller: std::sync::Arc<
            tokio::sync::Mutex<Option<std::sync::Arc<crate::p2p_vpn::P2PVpnController>>>,
        > = std::sync::Arc::new(tokio::sync::Mutex::new(None));

        #[cfg(feature = "vpn")]
        {
            let args_clone = args.clone();
            let room_id = args.room.clone().unwrap_or_else(|| "default".to_string());
            let vpn_controller_clone = vpn_controller.clone();
            let shutdown_tx_clone = shutdown_tx.clone();

            rt.spawn(async move {
                match crate::start_p2p_vpn(args_clone, room_id).await {
                    Ok(vpn) => {
                        info!("VPN started in background");
                        let mut guard = vpn_controller_clone.lock().await;
                        // start_p2p_vpn returns P2PVpnController (struct), not Arc.
                        // But I changed it to return Arc? No, I changed it to return P2PVpnController.
                        // Wait, let me check start_p2p_vpn signature in main.rs again.
                        // It returns `Result<p2p_vpn::P2PVpnController, BoxError>`.
                        // So I should wrap it in Arc if I want to share it easily, or just store it.
                        // P2PVpnController is cheap to clone? It has Arc internals?
                        // Let's assume I can wrap it in Arc.
                        *guard = Some(std::sync::Arc::new(vpn));
                    }
                    Err(e) => {
                        error!("Failed to start VPN: {}", e);
                        let _ = shutdown_tx_clone.send(());
                    }
                }
            });
        }

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        info!("ZKS VPN Service started");

        // Wait for stop signal
        let _ = shutdown_rx.recv();

        info!("ZKS VPN Service stopping");

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StopPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        #[cfg(feature = "vpn")]
        {
            rt.block_on(async {
                let guard = vpn_controller.lock().await;
                if let Some(vpn) = &*guard {
                    info!("Stopping VPN controller...");
                    if let Err(e) = vpn.stop().await {
                        error!("Failed to stop VPN cleanly: {}", e);
                    }
                }
            });
        }

        // Shutdown runtime
        rt.shutdown_timeout(Duration::from_secs(5));

        status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }

    pub fn install_service() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let exe_path = std::env::current_exe()?;

        let launch_arguments = vec![OsString::from("--service")];

        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path,
            launch_arguments,
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };

        service_manager.create_service(&service_info, ServiceAccess::DELETE)?;

        info!("Service installed successfully");
        Ok(())
    }

    pub fn uninstall_service() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

        let service_access = ServiceAccess::DELETE;
        let service = service_manager.open_service(SERVICE_NAME, service_access)?;

        service.delete()?;

        info!("Service uninstalled successfully");
        Ok(())
    }
}
