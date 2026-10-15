pub mod browser;
mod commands;
mod platform;
mod theme;
mod window;

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use tauri::{App, AppHandle, Manager, RunEvent, WindowEvent};
use zvs_host::browser::{BrowserLifecycle, profile_dir};
use zvs_host::data::repositories::SecretFilter;
use zvs_host::data::{MigrationFailed, PrepareOptions, prepare_database};
use zvs_host::drivers::ai::adapters::FileSystemModelStore;
use zvs_host::drivers::ai::{
    DatabaseAccounts, DatabaseProviders, HttpClient, ProviderRegistry, ProviderRegistryOptions,
};
use zvs_host::drivers::core::InProcessCore;
use zvs_host::drivers::sidecar::{SidecarDriver, SidecarOptions};
use zvs_host::platform::clock::{SharedClock, SystemClock};
use zvs_host::platform::device::{DeviceProbe, DeviceProbeOptions};
use zvs_host::platform::disk::free_bytes;
use zvs_host::platform::events::{EventBus, EventBusOptions, EventRecorder};
use zvs_host::platform::ids::system_ids;
use zvs_host::platform::logger::{FileLogger, LogLevel, LoggerOptions, SharedLogger};
use zvs_host::platform::metrics::nvidia_monitor;
use zvs_host::platform::navigation::SharedNavigator;
use zvs_host::platform::paths::{PathEnvironment, StudioPaths};
use zvs_host::platform::recorder::{FileEventRecorder, recording_enabled_from_env};
use zvs_host::platform::shutdown::{
    DEFAULT_SHUTDOWN_CAP, ExitDecision, ShutdownCoordinator, ShutdownStage,
};
use zvs_host::platform::theme::{SharedThemePort, THEME_SETTING_KEY, apply_theme_source};
use zvs_host::platform::window_state::{WINDOW_STATE_KEY, WindowState, read_window_state};
use zvs_host::services::system::DEMO_INTERVAL;
use zvs_host::services::{
    AccountService, AccountServiceOptions, CryptoService, HealthCheckOptions, HealthCheckService,
    ProviderService, ProviderServiceOptions, SecretService, Services, SettingService,
    SystemService,
};
use zvs_host::{IpcServer, ipc};

use crate::browser::gateway::GatewayOptions;
use crate::browser::{
    AccountGateway, BearerTokens, BrowserWorkspace, WebviewPages, WorkspaceOpener, gateway_sessions,
};
use crate::commands::Host;
use crate::theme::TauriTheme;

enum Boot {
    Studio {
        host: Host,
        window: WindowState,
        logger: SharedLogger,
        lifecycle: Lifecycle,
        browser: Browser,
    },
    Recovery(MigrationFailed),
}

struct Browser {
    workspace: BrowserWorkspace,
    gateway: Arc<AccountGateway>,
    events: BrowserLifecycle,
}

struct Lifecycle {
    shutdown: Arc<ShutdownCoordinator>,
    logger: SharedLogger,
    events: EventBus,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::focus_studio(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::ipc_call,
            commands::events_subscribe,
            commands::browser_command,
            commands::browser_subscribe
        ])
        .setup(|app| {
            match compose(app)? {
                Boot::Studio {
                    host,
                    window,
                    logger,
                    lifecycle,
                    browser,
                } => {
                    let services = Arc::clone(&host.services);
                    let gateway = browser.gateway;
                    #[cfg(feature = "e2e")]
                    probe_gateway(Arc::clone(&gateway), logger.clone());
                    app.manage(browser.workspace);
                    app.manage(Arc::clone(&gateway));
                    app.manage(browser.events);
                    app.manage(host);
                    app.manage(lifecycle);
                    window::create_studio(app.handle(), window, move |state| {
                        let stored = serde_json::to_value(state)
                            .map_err(|error| error.to_string())
                            .and_then(|value| {
                                services
                                    .settings
                                    .set(WINDOW_STATE_KEY, &value)
                                    .map_err(|error| error.to_string())
                            });
                        if let Err(error) = stored {
                            logger.log(
                                LogLevel::Warn,
                                "window",
                                "Could not store the window geometry",
                                &[("error", json!(error))],
                            );
                        }
                    })?;
                    if let Some(studio) = app.get_window(window::STUDIO) {
                        studio.on_window_event(move |event| {
                            if let WindowEvent::Destroyed = event {
                                gateway.dispose();
                            }
                        });
                    }
                }
                Boot::Recovery(failure) => window::create_recovery(app.handle(), &failure)?,
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("ZVS AI Studio failed to start")
        .run(on_run_event);
}

#[cfg(feature = "e2e")]
fn probe_gateway(gateway: Arc<AccountGateway>, logger: SharedLogger) {
    use zvs_host::drivers::ai::{SessionGateway, SessionRequest};

    let Ok(url) = std::env::var("ZVS_E2E_GATEWAY_PROBE") else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        loop {
            let mut request = SessionRequest::new("GET").with_header("accept", "application/json");
            let bearer = gateway.account_token(&url);
            if let Some(token) = &bearer {
                request = request.with_header("authorization", token.authorization());
            }
            let outcome = gateway
                .fetch(&url, request, &tokio_util::sync::CancellationToken::new())
                .await;
            logger.log(
                LogLevel::Info,
                "browser",
                "Gateway probe",
                &[
                    ("url", json!(url)),
                    ("bearer", json!(bearer.is_some())),
                    (
                        "status",
                        json!(outcome.as_ref().ok().map(|response| response.status)),
                    ),
                    (
                        "error",
                        json!(outcome.as_ref().err().map(|error| error.code)),
                    ),
                ],
            );
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    });
}

fn warm_sidecar(sidecar: SidecarDriver, logger: SharedLogger) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = sidecar.ping().await {
            logger.log(
                LogLevel::Warn,
                "sidecar",
                "The sidecar is unavailable",
                &[("code", json!(error.code)), ("error", json!(error.message))],
            );
        }
    });
}

fn on_run_event(app: &AppHandle, event: RunEvent) {
    let Some(lifecycle) = app.try_state::<Lifecycle>() else {
        return;
    };
    match event {
        RunEvent::ExitRequested { api, .. } => match lifecycle.shutdown.request_exit() {
            ExitDecision::Begin => {
                api.prevent_exit();
                let shutdown = Arc::clone(&lifecycle.shutdown);
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    shutdown.run().await;
                    app.exit(0);
                });
            }
            ExitDecision::Wait => api.prevent_exit(),
            ExitDecision::Exit => {}
        },
        RunEvent::Exit => {
            lifecycle
                .logger
                .log(LogLevel::Info, "host", "Shutdown", &[]);
            lifecycle.events.dispose();
        }
        _ => {}
    }
}

fn compose(app: &App) -> Result<Boot, Box<dyn Error>> {
    let paths = StudioPaths::resolve(&PathEnvironment {
        data_dir: app.path().app_data_dir()?,
        resource_dir: app.path().resource_dir()?,
        executable_dir: std::env::current_exe()?
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        packaged: !tauri::is_dev(),
    });
    let clock: SharedClock = Arc::new(SystemClock);

    let mut options = LoggerOptions::new(paths.logs_dir.clone(), LogLevel::Info, clock.clone());
    options.development = !paths.packaged;
    let logger: SharedLogger = Arc::new(FileLogger::open(options)?);
    logger.log(
        LogLevel::Info,
        "host",
        "Startup",
        &[("version", json!(app.package_info().version.to_string()))],
    );

    let (database, report) = match prepare_database(&PrepareOptions {
        file: &paths.db_path,
        backups_dir: &paths.backups_dir,
        logger: logger.clone(),
        clock: clock.clone(),
    }) {
        Ok(prepared) => prepared,
        Err(failure) => return Ok(Boot::Recovery(failure)),
    };
    logger.log(
        LogLevel::Info,
        "host",
        "Database ready",
        &[("applied", json!(report.applied.len()))],
    );
    let database = Arc::new(database);

    let theme: SharedThemePort = Arc::new(TauriTheme::new(app.handle().clone()));
    let settings = SettingService::new(Arc::clone(&database), clock.clone());
    apply_theme_source(
        settings
            .get(THEME_SETTING_KEY)?
            .map(|setting| setting.value)
            .as_ref(),
        theme.as_ref(),
    );
    let window = read_window_state(
        settings
            .get(WINDOW_STATE_KEY)?
            .map(|setting| setting.value)
            .as_ref(),
    );

    let crypto = Arc::new(CryptoService::new(platform::crypto_backend()));
    let secrets = Arc::new(SecretService::new(
        Arc::clone(&database),
        crypto,
        clock.clone(),
        logger.clone(),
    ));
    logger.log(
        LogLevel::Info,
        "host",
        "Secret store ready",
        &[
            ("encryption", json!(secrets.is_encryption_available())),
            (
                "count",
                json!(secrets.list(&SecretFilter::default())?.len()),
            ),
        ],
    );

    let recording = recording_enabled_from_env();
    let recorder: Option<Box<dyn EventRecorder>> = if recording {
        Some(Box::new(FileEventRecorder::open(
            &paths.streams_dir,
            logger.clone(),
            clock.clone(),
        )?))
    } else {
        None
    };
    let events = EventBus::new(EventBusOptions {
        logger: logger.clone(),
        clock: clock.clone(),
        ids: system_ids(),
        recorder,
    });
    logger.log(
        LogLevel::Info,
        "host",
        "Opened the event channel",
        &[("recording", json!(recording))],
    );

    let browser_events = BrowserLifecycle::new();
    let profile = profile_dir(&app.path().app_data_dir()?);
    let tokens = BearerTokens::default();
    let navigator: SharedNavigator = Arc::new(events.clone());
    let workspace = BrowserWorkspace::new(
        app.handle().clone(),
        profile.clone(),
        browser_events.clone(),
        tokens.clone(),
        navigator.clone(),
    );
    let gateway = Arc::new(AccountGateway::with_options(
        Arc::new(WebviewPages::new(app.handle().clone(), profile)),
        tokens,
        GatewayOptions::default(),
        logger.clone(),
    ));
    let sessions = gateway_sessions(Arc::clone(&gateway));

    let mut registry = ProviderRegistryOptions::new(
        Arc::new(DatabaseProviders::new(Arc::clone(&database))),
        secrets.clone(),
        HttpClient::new()?,
    );
    registry.accounts = Arc::new(DatabaseAccounts::new(
        Arc::clone(&database),
        Some(sessions.clone()),
        secrets.clone(),
        logger.clone(),
    ));
    registry.local_models = Some(Arc::new(FileSystemModelStore::new(
        paths.downloads_dir.clone(),
    )));
    registry.logger = logger.clone();
    let providers = ProviderService::new(ProviderServiceOptions::new(
        Arc::clone(&database),
        Arc::new(ProviderRegistry::new(registry)),
        Arc::clone(&secrets),
        logger.clone(),
        clock.clone(),
    ));
    let runtime = tauri::async_runtime::handle().inner().clone();
    let accounts = AccountService::new(AccountServiceOptions {
        clock: clock.clone(),
        logger: logger.clone(),
        runtime: Some(runtime.clone()),
        ..AccountServiceOptions::new(
            Arc::clone(&database),
            Arc::clone(&secrets),
            sessions,
            Arc::new(WorkspaceOpener(workspace.clone())),
            browser_events.clone(),
            events.clone(),
            navigator,
        )
    });
    let health = HealthCheckService::new(HealthCheckOptions {
        providers: providers.clone(),
        settings: settings.clone(),
        logger: logger.clone(),
        clock: clock.clone(),
        events: Some(events.clone()),
        runtime: Some(runtime),
    });

    let sidecar = SidecarDriver::new(SidecarOptions {
        logger: logger.clone(),
        ..SidecarOptions::new(paths.sidecar_path.clone())
    });
    let device = Arc::new(DeviceProbe::new(DeviceProbeOptions {
        gpu: Some(platform::gpu_probe()),
        free_disk: Some({
            let downloads = paths.downloads_dir.clone();
            Arc::new(move || free_bytes(&downloads))
        }),
        logger: logger.clone(),
        ..DeviceProbeOptions::new(clock.clone())
    }));
    let resources = Arc::new(nvidia_monitor(clock.clone(), logger.clone()));
    let system = SystemService::new(clock.clone(), events.clone(), DEMO_INTERVAL)
        .with_core(InProcessCore::shared())
        .with_device(device)
        .with_resources(resources);

    let shutdown = Arc::new(ShutdownCoordinator::new(
        logger.clone(),
        DEFAULT_SHUTDOWN_CAP,
    ));
    {
        let health = health.clone();
        shutdown.register(
            ShutdownStage::Stop,
            "health-check.stop",
            move || async move {
                health.stop();
            },
        );
    }
    {
        let providers = providers.clone();
        shutdown.register(ShutdownStage::Dispose, "providers", move || async move {
            providers.dispose().await;
        });
    }
    {
        let health = health.clone();
        shutdown.register(ShutdownStage::Dispose, "health-check", move || async move {
            health.dispose().await;
        });
    }
    {
        let accounts = accounts.clone();
        shutdown.register(ShutdownStage::Dispose, "accounts", move || async move {
            accounts.dispose().await;
        });
    }
    {
        let gateway = Arc::clone(&gateway);
        shutdown.register(
            ShutdownStage::Dispose,
            "account-gateway",
            move || async move {
                gateway.dispose();
            },
        );
    }
    {
        let sidecar = sidecar.clone();
        shutdown.register(ShutdownStage::Dispose, "sidecar", move || async move {
            sidecar.dispose().await;
        });
    }

    let services = Arc::new(Services {
        system,
        accounts,
        settings,
        secrets,
        providers,
        health,
        theme,
    });
    let mut server = IpcServer::new(Arc::clone(&services), logger.clone());
    ipc::register(&mut server);
    server.finish();
    services.accounts.start_auto_refresh()?;
    services.health.start();
    warm_sidecar(sidecar, logger.clone());

    Ok(Boot::Studio {
        host: Host {
            server,
            events: events.clone(),
            services,
        },
        window,
        logger: logger.clone(),
        lifecycle: Lifecycle {
            shutdown,
            logger,
            events,
        },
        browser: Browser {
            workspace,
            gateway,
            events: browser_events,
        },
    })
}
