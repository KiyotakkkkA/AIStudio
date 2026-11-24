pub mod browser;
mod cli;
mod commands;
mod computer;
mod dev_app;
mod e2e;
mod picker;
mod platform;
mod shell;
mod studio_chrome;
mod theme;
mod toast;
mod window;

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde_json::json;
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use zvs_contract::{NOTIFICATION_SCHEME, NotificationTargetDto};
use zvs_host::agentic::{AgenticService, AgenticServiceOptions, register_agentic_nodes};
use zvs_host::artifacts::{ArtifactStore, DASHBOARDS};
use zvs_host::background::{BackgroundShellOptions, BackgroundShells, register_background_nodes};
use zvs_host::boot::{Boot as StateBoot, BootDecision, BootError, LegacySources, StateRootLock};
use zvs_host::browser::{
    AgentControls, Browser as BrowserTools, BrowserAgent, BrowserDownloads, BrowserExtensions,
    BrowserGrants, BrowserImporter, BrowserLibrary, BrowserLifecycle, BrowserSettings, ImportRoots,
    RoutedPages, register_browser_nodes, setting_limit, tool_switch,
};
use zvs_host::commands::{CommandToolOptions, register_command_nodes};
use zvs_host::computer::{
    Computer, ComputerAgent, ComputerControls, UvSidecar, register_computer_nodes,
};
use zvs_host::dashboards::ProcessDashboardRunner;
use zvs_host::data::Repositories;
use zvs_host::data::repositories::{ProviderFilter, SecretFilter};
use zvs_host::data::{MigrationFailed, PrepareOptions, prepare_database};
use zvs_host::documents::{DocumentService, DocumentServiceOptions};
use zvs_host::downloads::{
    CatalogueService, CatalogueServiceOptions, CuratedProvider, DiskService, DiskServiceOptions,
    DownloadService, DownloadServiceOptions, OllamaProvider,
};
use zvs_host::drivers::ai::adapters::FileSystemModelStore;
use zvs_host::drivers::ai::{
    DatabaseAccounts, DatabaseProviders, HttpClient, HttpClientOptions, ProviderRegistry,
    ProviderRegistryOptions, ProxyMode,
};
use zvs_host::drivers::core::InProcessCore;
use zvs_host::drivers::mcp::ToolchainDirs;
use zvs_host::drivers::sidecar::{SidecarDriver, SidecarOptions};
use zvs_host::files::{FileToolOptions, VirtualRoots, register_file_nodes};
use zvs_host::indexing::{
    DocumentExtractorOptions, ImageExtractor, IndexingService, IndexingServiceOptions,
    PdfExtractor, image_extensions,
};
use zvs_host::integrations::git::GitHostingProvider;
use zvs_host::integrations::mail::MailProvider;
use zvs_host::integrations::telegram::TelegramProvider;
use zvs_host::integrations::{IntegrationService, IntegrationServiceOptions};
use zvs_host::kernel::{
    KernelServices, NodeRegistry, register_core_nodes, register_download_nodes, register_job_nodes,
};
use zvs_host::notifications::{NotificationService, NotificationServiceOptions};
use zvs_host::platform::clock::{Clock, SharedClock, SystemClock};
use zvs_host::platform::device::{DeviceProbe, DeviceProbeOptions};
use zvs_host::platform::disk::free_bytes;
use zvs_host::platform::events::{EventBus, EventBusOptions, EventRecorder};
use zvs_host::platform::ids::system_ids;
use zvs_host::platform::logger::{FileLogger, LogLevel, LoggerOptions, SharedLogger};
use zvs_host::platform::metrics::nvidia_monitor;
use zvs_host::platform::navigation::SharedNavigator;
use zvs_host::platform::paths::{PathEnvironment, SharedPaths, StudioLayout, StudioPaths};
use zvs_host::platform::recorder::{FileEventRecorder, recording_enabled_from_env};
use zvs_host::platform::shell::SharedShellPort;
use zvs_host::platform::shutdown::{
    DEFAULT_SHUTDOWN_CAP, ExitDecision, ShutdownCoordinator, ShutdownStage,
};
use zvs_host::platform::theme::{
    FONT_SIZE_SETTING_KEY, PaletteMode, SharedThemePort, THEME_SETTING_KEY, apply_interface_zoom,
    apply_palette, apply_theme_source,
};
use zvs_host::platform::window_state::{WINDOW_STATE_KEY, WindowState, read_window_state};
use zvs_host::policy::{ExecPolicyOptions, ExecPolicyService, SealedPackKeys};
use zvs_host::runtimes::{
    ReleaseResolver, ReleaseResolverOptions, RuntimeService, RuntimeServiceOptions,
};
use zvs_host::scenarios::{
    EffectsSlot, HostScenarioEffects, ScenarioNodeOptions, ScenarioRunner, ScenarioRunnerOptions,
    ScenarioService, ScenarioServiceOptions, register_scenario_nodes,
};
use zvs_host::services::rerank::local_rerankers;
use zvs_host::services::shared_credentials::SharedCredentials;
use zvs_host::services::system::DEMO_INTERVAL;
use zvs_host::services::{
    AccountService, AccountServiceOptions, ChatService, ChatServiceOptions, CryptoService,
    DriverSource, GraphService, GraphServiceOptions, HealthCheckOptions, HealthCheckService,
    McpService, McpServiceOptions, ProviderService, ProviderServiceOptions, RetentionService,
    RetentionServiceOptions, RunService, RunServiceOptions, SecretService, Services,
    SettingService, StorageRoots, StorageService, SubagentService, SystemService,
    VectorEvalService, VectorEvalServiceOptions, VectorSourceService, VectorSourceServiceOptions,
    VectorStoreService, VectorStoreServiceOptions,
};
use zvs_host::skills::{
    SkillLibrary, SkillLibraryOptions, SkillService, SkillServiceOptions, register_skill_nodes,
};
use zvs_host::terminal::{NativePty, SystemShells, TerminalService, TerminalServiceOptions};
use zvs_host::toolchain::{
    ToolchainResolver, ToolchainResolverOptions, ToolchainService, ToolchainServiceOptions,
};
use zvs_host::tools::{ToolRegistry, ToolRegistryOptions, register_kernel_tools};
use zvs_host::voice::whisper::WhisperCli;
use zvs_host::voice::{VoiceService, VoiceServiceOptions};
use zvs_host::workers::WorkerSupervisor;
use zvs_host::workspace::{ExplorerService, FolderAccessService, ForbiddenFolders};
use zvs_host::{IpcServer, ipc};

use crate::browser::gateway::GatewayOptions;
use crate::browser::{
    AccountGateway, BearerTokens, BrowserWorkspace, WebviewPages, WorkspaceOpener, WorkspacePages,
    WorkspaceProfile, gateway_sessions, workspace_url_opener,
};
use crate::commands::Host;
use crate::dev_app::DevApp;
use crate::picker::DialogPicker;
use crate::shell::{ShellState, TauriShell};
use crate::theme::TauriTheme;
use zvs_host::platform::picker::SharedPicker;

enum Launch {
    Studio {
        host: Host,
        window: WindowState,
        hidden: bool,
        shell: Arc<ShellState>,
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
    computer: computer::ComputerShell,
}

struct Lifecycle {
    restart: std::sync::atomic::AtomicBool,
    shutdown: Arc<ShutdownCoordinator>,
    logger: SharedLogger,
    events: EventBus,
}

const _: () = assert!(
    !cfg!(feature = "e2e") || cfg!(debug_assertions) || option_env!("ZVS_E2E_BUILD").is_some(),
    "the e2e feature ships only in debug builds or builds made by the e2e launcher"
);

pub fn run() {
    e2e::clear_webview2_variables_before_any_thread();
    let builder = tauri::Builder::default();
    let builder = if e2e::data_dir().is_some() {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            open_launch_link(app, &args);
        }))
    };
    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(shell::AUTOSTART_ARG)
                .app_name(APP_NAME)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::ipc_call,
            commands::events_subscribe,
            commands::browser_command,
            commands::browser_subscribe,
            commands::computer_command,
            commands::computer_subscribe,
            commands::computer_pointer
        ])
        .setup(|app| start(app.handle()))
        .build(tauri::generate_context!())
        .expect("AI Studio Desktop failed to start")
        .run(on_run_event);
}

fn resolve_paths(
    app: &AppHandle,
) -> Result<(StudioPaths, StudioLayout, Option<LegacySources>), Box<dyn Error>> {
    let packaged = !tauri::is_dev();
    let (layout, legacy) = match e2e::data_dir() {
        Some(dir) => (StudioLayout::isolated(dir), None),
        None => (
            StudioLayout::resolve(&local_data_dir(app)?, !packaged),
            Some(LegacySources {
                roaming: app.path().app_data_dir()?,
                local: app.path().app_local_data_dir().ok(),
            }),
        ),
    };
    let paths = StudioPaths::resolve(&PathEnvironment {
        state_dir: layout.desktop.clone(),
        resource_dir: app.path().resource_dir()?,
        executable_dir: std::env::current_exe()?
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        packaged,
    })
    .with_shared(&layout.shared);
    Ok((paths, layout, legacy))
}

#[cfg(windows)]
fn local_data_dir(_app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    Ok(zvs_contract::layout::local_data_dir()?)
}

#[cfg(not(windows))]
fn local_data_dir(app: &AppHandle) -> Result<PathBuf, Box<dyn Error>> {
    Ok(app.path().local_data_dir()?)
}

fn setup_migration_profile(app: &AppHandle) -> std::io::Result<()> {
    let profile = tempfile::tempdir()?;
    window::set_shell_profile(profile.path().to_path_buf());
    app.manage(profile);
    Ok(())
}

fn boot_failure(
    error: &BootError,
    paths: &StudioPaths,
    legacy: Option<&LegacySources>,
) -> MigrationFailed {
    let preserved = match error {
        BootError::Io(_) | BootError::Mismatch(_) => legacy
            .filter(|legacy| legacy.present())
            .map_or_else(|| paths.root.clone(), |legacy| legacy.roaming.clone()),
        _ => paths.root.clone(),
    };
    MigrationFailed {
        message: error.to_string(),
        backups_dir: preserved,
    }
}

fn start(app: &AppHandle) -> Result<(), Box<dyn Error>> {
    let (paths, layout, legacy) = resolve_paths(app)?;
    let boot = StateBoot::new(&paths.root, legacy.as_ref()).with_layout(&layout);
    let reset_root = match boot.working_root() {
        Ok(root) => root,
        Err(error) => {
            setup_migration_profile(app)?;
            window::create_recovery(app, &boot_failure(&error, &paths, legacy.as_ref()))?;
            return Ok(());
        }
    };
    if reset_root
        .join(zvs_host::services::storage::RESET_MARKER)
        .try_exists()?
        && e2e::data_dir().is_none()
    {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().disable()?;
    }
    // WebView2 can briefly retain profile files after the previous process exits.
    for attempt in 0..=20 {
        use zvs_host::services::storage::{
            apply_pending_reset, apply_pending_reset_before_relocation,
        };
        let reset = if reset_root == paths.root {
            apply_pending_reset(reset_root)
        } else {
            apply_pending_reset_before_relocation(reset_root)
        };
        let state = reset.and_then(|_| StateRootLock::shared(&paths.root));
        match state {
            Ok(lock) => {
                app.manage(lock);
                break;
            }
            Err(error) if attempt == 20 && error.kind() == std::io::ErrorKind::WouldBlock => {
                setup_migration_profile(app)?;
                window::create_recovery(
                    app,
                    &boot_failure(&BootError::StateInUse, &paths, legacy.as_ref()),
                )?;
                return Ok(());
            }
            Err(error) if attempt == 20 => return Err(error.into()),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
    match DevApp::current() {
        DevApp::Boot => {
            window::set_shell_profile(paths.boot_profile_dir.clone());
            window::create_boot(app, true)?;
            dev_app::play_boot_preview(app.clone())?;
            return Ok(());
        }
        DevApp::Recovery => {
            window::set_shell_profile(paths.boot_profile_dir.clone());
            let failure = boot_failure(&dev_app::recovery_preview(), &paths, legacy.as_ref());
            window::create_recovery(app, &failure)?;
            return Ok(());
        }
        DevApp::Studio | DevApp::Browser | DevApp::Computer => {}
    }
    let decision = match boot.inspect() {
        Ok(decision) => decision,
        Err(error) => {
            setup_migration_profile(app)?;
            window::create_recovery(app, &boot_failure(&error, &paths, legacy.as_ref()))?;
            return Ok(());
        }
    };
    match decision {
        BootDecision::Migrate(_) => {
            setup_migration_profile(app)?;
            migrate_then_launch(app, paths, layout, legacy, decision)
        }
        _ => {
            window::set_shell_profile(paths.shell_profile_dir.clone());
            if let Err(error) = boot.run(&decision, SystemClock.now(), &|_| {}) {
                window::create_recovery(app, &boot_failure(&error, &paths, legacy.as_ref()))?;
                return Ok(());
            }
            let packaged = paths.packaged;
            let root = paths.root.clone();
            launch(app, paths, &layout.shared)?;
            if packaged && let Some(legacy) = legacy {
                std::thread::spawn(move || {
                    zvs_host::boot::sweep_legacy(&root, &legacy, SystemClock.now());
                });
            }
            Ok(())
        }
    }
}

fn migrate_then_launch(
    app: &AppHandle,
    paths: StudioPaths,
    layout: StudioLayout,
    legacy: Option<LegacySources>,
    decision: BootDecision,
) -> Result<(), Box<dyn Error>> {
    window::create_boot(app, false)?;
    let app = app.clone();
    std::thread::Builder::new()
        .name("state-migration".to_owned())
        .spawn(move || {
            let reporter = window::BootReporter::new(app.clone());
            let outcome = StateBoot::new(&paths.root, legacy.as_ref())
                .with_layout(&layout)
                .run(&decision, SystemClock.now(), &|progress| {
                    reporter.report(progress)
                });
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let failure = match outcome {
                    Ok(()) => {
                        window::set_shell_profile(paths.shell_profile_dir.clone());
                        launch(&handle, paths, &layout.shared)
                            .err()
                            .map(|error| MigrationFailed {
                                message: error.to_string(),
                                backups_dir: PathBuf::new(),
                            })
                    }
                    Err(error) => Some(boot_failure(&error, &paths, legacy.as_ref())),
                };
                if let Some(failure) = failure {
                    let _ = window::create_recovery(&handle, &failure);
                }
                window::close_boot(&handle);
            });
        })?;
    Ok(())
}

fn launch(app: &AppHandle, paths: StudioPaths, shared: &SharedPaths) -> Result<(), Box<dyn Error>> {
    zvs_host::boot::shared::register_desktop(shared, &std::env::current_exe()?)?;
    match compose(app, paths, shared)? {
        Launch::Studio {
            host,
            window,
            hidden,
            shell,
            logger,
            lifecycle,
            browser,
        } => {
            let services = Arc::clone(&host.services);
            let zoom_services = Arc::clone(&services);
            let gateway = browser.gateway;
            #[cfg(feature = "e2e")]
            probe_gateway(Arc::clone(&gateway), logger.clone());
            let launched = DevApp::current();
            let overlay = browser.computer.clone();
            app.manage(browser.workspace);
            app.manage(browser.computer);
            app.manage(Arc::clone(&gateway));
            app.manage(browser.events);
            app.manage(host);
            app.manage(lifecycle);
            let page = match launched {
                DevApp::Browser => Some(window::BROWSER_ONLY_PAGE),
                _ => None,
            };
            let persist = move |state: WindowState| {
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
            };
            if launched == DevApp::Computer {
                overlay.preview();
            } else {
                window::create_studio(app, window, hidden, page, shell, persist)?;
            }
            if let Ok(stored) = zoom_services.settings.get(FONT_SIZE_SETTING_KEY) {
                apply_interface_zoom(
                    stored.map(|setting| setting.value).as_ref(),
                    zoom_services.theme.as_ref(),
                );
            }
            if let Ok(stored) = zoom_services.settings.get(THEME_SETTING_KEY) {
                apply_theme_source(
                    stored.map(|setting| setting.value).as_ref(),
                    zoom_services.theme.as_ref(),
                );
            }
            for mode in PaletteMode::ALL {
                if let Ok(stored) = zoom_services.settings.get(mode.setting_key()) {
                    apply_palette(
                        mode.setting_key(),
                        stored.map(|setting| setting.value).as_ref(),
                        zoom_services.theme.as_ref(),
                    );
                }
            }
            if let Some(studio) = app.get_window(window::STUDIO) {
                studio.on_window_event(move |event| {
                    if let WindowEvent::Destroyed = event {
                        gateway.dispose();
                    }
                });
            }
        }
        Launch::Recovery(failure) => window::create_recovery(app, &failure)?,
    }
    Ok(())
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

fn warm_account_pages(
    accounts: &AccountService,
    gateway: Arc<AccountGateway>,
    logger: SharedLogger,
) {
    let Ok(accounts) = accounts.list() else {
        return;
    };
    let origins: Vec<&'static str> = [
        (
            zvs_contract::AccountFamily::QwenWeb,
            zvs_qwen_web::protocol::ORIGIN,
        ),
        (
            zvs_contract::AccountFamily::ZaiWeb,
            zvs_zai_web::protocol::ORIGIN,
        ),
    ]
    .into_iter()
    .filter(|(family, _)| {
        accounts.iter().any(|account| {
            account.adapter == *family && account.status == zvs_contract::AccountStatus::Linked
        })
    })
    .map(|(_, origin)| origin)
    .collect();
    if origins.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        for origin in origins {
            if let Err(error) = gateway.warm(origin).await {
                logger.log(
                    LogLevel::Info,
                    "browser",
                    "Could not warm the account gateway",
                    &[("origin", json!(origin)), ("code", json!(error.code))],
                );
            }
        }
    });
}

fn open_launch_link(app: &AppHandle, args: &[String]) {
    if cli::is_cli_launch(args) {
        return;
    }
    let Some(url) = toast::launch_url(args) else {
        window::focus_studio(app);
        return;
    };
    let Some(target) = toast::launch_target(url) else {
        if let Some(lifecycle) = app.try_state::<Lifecycle>() {
            reject_launch_link(&lifecycle.logger, url);
        }
        return;
    };
    match app
        .try_state::<toast::ActivationSlot>()
        .and_then(|slot| slot.get().cloned())
    {
        Some(activate) => activate(target),
        None => window::focus_studio(app),
    }
}

fn cold_launch_target(logger: &SharedLogger) -> Option<NotificationTargetDto> {
    let args: Vec<String> = std::env::args().collect();
    let url = toast::launch_url(&args)?;
    let target = toast::launch_target(url);
    if target.is_none() {
        reject_launch_link(logger, url);
    }
    target
}

fn reject_launch_link(logger: &SharedLogger, url: &str) {
    logger.log(
        LogLevel::Warn,
        "shell",
        "Ignored a malformed zvs-studio link",
        &[("length", json!(url.len()))],
    );
}

fn register_link_scheme(app: &AppHandle, logger: &SharedLogger) {
    if let Err(error) = app.deep_link().register(NOTIFICATION_SCHEME) {
        logger.log(
            LogLevel::Warn,
            "shell",
            "Could not register the zvs-studio scheme",
            &[("error", json!(error.to_string()))],
        );
    }
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
                let restart = lifecycle.restart.load(std::sync::atomic::Ordering::SeqCst);
                let shutdown = Arc::clone(&lifecycle.shutdown);
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    shutdown.run().await;
                    if restart {
                        app.request_restart();
                    } else {
                        app.exit(0);
                    }
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

const APP_NAME: &str = "AI Studio Desktop";
fn compose(
    app: &AppHandle,
    paths: StudioPaths,
    shared: &SharedPaths,
) -> Result<Launch, Box<dyn Error>> {
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

    let (mut database, report) = match prepare_database(&PrepareOptions {
        file: &paths.db_path,
        backups_dir: &paths.backups_dir,
        logger: logger.clone(),
        clock: clock.clone(),
    }) {
        Ok(prepared) => prepared,
        Err(failure) => return Ok(Launch::Recovery(failure)),
    };
    let dialog_recovery = zvs_host::services::data_transfer::recover(&paths, &database)
        .map_err(zvs_host::data::DataError::from)
        .and_then(|()| database.enable_dialog_history(paths.conversations_dir.clone()));
    if let Err(error) = dialog_recovery {
        logger.log(
            LogLevel::Error,
            "host",
            "Could not rebuild dialog history",
            &[("error", json!(error.to_string()))],
        );
        return Ok(Launch::Recovery(MigrationFailed {
            message: "Не удалось восстановить историю диалогов".to_owned(),
            backups_dir: paths.backups_dir.clone(),
        }));
    }
    logger.log(
        LogLevel::Info,
        "host",
        "Database ready",
        &[("applied", json!(report.applied.len()))],
    );
    let swept = zvs_host::boot::sweep_temp(&[paths.staging_dir.clone(), paths.runs_dir.clone()]);
    if swept > 0 {
        logger.log(
            LogLevel::Info,
            "host",
            "Cleared temporary files",
            &[("count", json!(swept))],
        );
    }
    let database = Arc::new(database);
    let data_transfer = zvs_host::services::data_transfer::DataTransferService::new(
        paths.clone(),
        Arc::clone(&database),
        Arc::clone(&clock),
    )?;

    let theme: SharedThemePort = Arc::new(TauriTheme::new(app.clone()));
    let shell_state = ShellState::shared();
    let activation: toast::ActivationSlot = Arc::new(OnceLock::new());
    app.manage(Arc::clone(&activation));
    if !paths.packaged && e2e::data_dir().is_none() {
        register_link_scheme(app, &logger);
    }
    let toast_icon = paths
        .notification_icon
        .is_file()
        .then(|| paths.notification_icon.clone());
    let toaster = platform::toaster(
        &app.config().identifier,
        APP_NAME,
        toast_icon.as_deref(),
        Arc::clone(&activation),
        logger.clone(),
    );
    let shell: SharedShellPort = Arc::new(TauriShell::new(
        app.clone(),
        Arc::clone(&shell_state),
        toaster,
    ));
    let settings = SettingService::new(Arc::clone(&database), clock.clone());
    let browser_settings = BrowserSettings::new(settings.clone());
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

    let crypto = Arc::new(CryptoService::new(zvs_host::services::crypto::os_backend()));
    let shared_credentials = Arc::new(SharedCredentials::new(shared, Arc::clone(&crypto)));
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
    let profile = paths.browser_profile_dir.clone();
    let tokens = BearerTokens::default();
    let navigator: SharedNavigator = Arc::new(events.clone());
    let agent = BrowserAgent::new(clock.clone());
    let workspace = BrowserWorkspace::new(
        app.clone(),
        profile.clone(),
        browser_events.clone(),
        tokens.clone(),
        navigator.clone(),
        agent.clone(),
    );
    let gateway = Arc::new(AccountGateway::with_options(
        Arc::new(WebviewPages::new(app.clone(), profile)),
        tokens,
        GatewayOptions::default(),
        logger.clone(),
    ));
    let sessions = gateway_sessions(Arc::clone(&gateway));
    let notices = zvs_host::cli::CliNotices::default();
    {
        let notices = notices.clone();
        gateway.watch_verification(Arc::new(move |origin, shown| {
            notices.verification(origin, shown);
        }));
    }
    {
        let gateway = Arc::clone(&gateway);
        notices.on_focus(move || gateway.focus_verification());
    }

    let http = HttpClient::new()?;
    let mcp_http = http.client().clone();
    let mut registry = ProviderRegistryOptions::new(
        Arc::new(DatabaseProviders::new(Arc::clone(&database))),
        secrets.clone(),
        http.clone(),
    );
    registry.accounts = Arc::new(
        DatabaseAccounts::new(
            Arc::clone(&database),
            Some(sessions.clone()),
            secrets.clone(),
            logger.clone(),
        )
        .with_openai_secrets(shared_credentials.clone()),
    );
    registry.local_models = Some(Arc::new(FileSystemModelStore::new(
        paths.assets_dir.clone(),
    )));
    registry.logger = logger.clone();
    let registry = Arc::new(ProviderRegistry::new(registry));
    let providers = ProviderService::new(ProviderServiceOptions::new(
        Arc::clone(&database),
        registry.clone(),
        Arc::clone(&secrets),
        logger.clone(),
        clock.clone(),
    ));
    let runtime = tauri::async_runtime::handle().inner().clone();
    let accounts = AccountService::new(AccountServiceOptions {
        clock: clock.clone(),
        logger: logger.clone(),
        runtime: Some(runtime.clone()),
        shared_credentials: Some(shared_credentials),
        ..AccountServiceOptions::new(
            Arc::clone(&database),
            Arc::clone(&secrets),
            sessions,
            Arc::new(WorkspaceOpener(workspace.clone(), browser_settings.clone())),
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
        runtime: Some(runtime.clone()),
    });

    let sidecar = SidecarDriver::new(SidecarOptions {
        logger: logger.clone(),
        ..SidecarOptions::new(paths.sidecar_path.clone())
    });
    let device = Arc::new(DeviceProbe::new(DeviceProbeOptions {
        gpu: Some(platform::gpu_probe()),
        free_disk: Some({
            let downloads = paths.assets_dir.clone();
            Arc::new(move || free_bytes(&downloads))
        }),
        logger: logger.clone(),
        ..DeviceProbeOptions::new(clock.clone())
    }));
    let resources = Arc::new(nvidia_monitor(clock.clone(), logger.clone()));
    let core = InProcessCore::shared();
    let vector_stores = VectorStoreService::new(VectorStoreServiceOptions::new(
        Arc::clone(&database),
        core.clone(),
        registry.clone(),
        paths.vector_stores_dir.clone(),
        logger.clone(),
        clock.clone(),
    ));
    let vector_sources = VectorSourceService::new(VectorSourceServiceOptions::new(
        Arc::clone(&database),
        vector_stores.clone(),
        logger.clone(),
        clock.clone(),
    ));
    let vector_eval = VectorEvalService::new(VectorEvalServiceOptions::new(
        Arc::clone(&database),
        Arc::new(vector_stores.clone()),
        logger.clone(),
        clock.clone(),
    ));
    let graph = GraphService::new(GraphServiceOptions {
        database: Arc::clone(&database),
        core: core.clone(),
        logger: logger.clone(),
        clock: clock.clone(),
    });
    let indexing = IndexingService::new(IndexingServiceOptions {
        resources: Some(Arc::new(Arc::clone(&resources))),
        ..IndexingServiceOptions::new(
            Arc::clone(&database),
            core.clone(),
            registry.clone(),
            vector_stores.clone(),
            logger.clone(),
            clock.clone(),
        )
    });
    let disk = DiskService::new(DiskServiceOptions::new(
        paths.assets_dir.clone(),
        paths.vector_stores_dir.clone(),
        paths.root.clone(),
        clock.clone(),
    ));
    let ollama_base_url = {
        let database = Arc::clone(&database);
        Arc::new(move || {
            database
                .with_connection(|connection| {
                    Ok(Repositories::new(connection)
                        .providers
                        .list(&ProviderFilter {
                            capability: None,
                            enabled: None,
                        })?)
                })
                .ok()?
                .into_iter()
                .find(|provider| {
                    provider.kind == zvs_contract::ProviderKind::Ollama && provider.enabled
                })
                .map(|provider| provider.base_url)
                .filter(|base_url| !base_url.is_empty())
        })
    };
    let catalogue = CatalogueService::new(CatalogueServiceOptions {
        providers: vec![
            Arc::new(CuratedProvider),
            Arc::new(OllamaProvider::new(
                ollama_base_url,
                http.clone(),
                logger.clone(),
            )),
        ],
        ..CatalogueServiceOptions::new(clock.clone(), logger.clone())
    });
    let downloads = DownloadService::new(DownloadServiceOptions {
        settings: Some(settings.clone()),
        staging_dir: Some(paths.temp_downloads_dir.clone()),
        stored: paths.stored_paths(),
        ..DownloadServiceOptions::new(
            Arc::clone(&database),
            Arc::new(sidecar.clone()),
            disk,
            catalogue.clone(),
            paths.assets_dir.clone(),
            logger.clone(),
            clock.clone(),
        )
    });
    let runtimes = RuntimeService::new(RuntimeServiceOptions {
        downloads: Arc::new(downloads.clone()),
        catalogue: Arc::new(catalogue.clone()),
        settings: settings.clone(),
        assets_dir: paths.assets_dir.clone(),
        device: Some(device.clone()),
        resolver: ReleaseResolver::new(ReleaseResolverOptions::new(
            http.clone(),
            clock.clone(),
            logger.clone(),
        )),
        http: HttpClient::with_options(HttpClientOptions {
            proxy: ProxyMode::Off,
            ..HttpClientOptions::default()
        })?,
        logger: logger.clone(),
        create_server: None,
        context_size: None,
    });
    downloads.attach_installer(Arc::new(runtimes.clone()));
    catalogue.add_provider(runtimes.catalogue_provider());
    let picker: SharedPicker = Arc::new(DialogPicker::new(app.clone()));
    let toolchain = ToolchainService::new(ToolchainServiceOptions {
        picker: Some(Arc::clone(&picker)),
        package_cache: Some(paths.package_cache_dir.clone()),
        ..ToolchainServiceOptions::new(
            paths.toolchain_dir.clone(),
            settings.clone(),
            Arc::new(downloads.clone()),
            Arc::new(catalogue.clone()),
            ToolchainResolver::new(ToolchainResolverOptions::new(
                http.clone(),
                clock.clone(),
                logger.clone(),
            )),
            logger.clone(),
        )
    });
    downloads.attach_installer(Arc::new(toolchain.clone()));
    catalogue.add_provider(toolchain.catalogue_provider());
    let voice = VoiceService::new(VoiceServiceOptions {
        settings: settings.clone(),
        downloads: downloads.clone(),
        audio: platform::audio_input(),
        device: Some(device.clone()),
        recognizer: Arc::new(WhisperCli),
        assets_dir: paths.assets_dir.clone(),
        cache_dir: paths.cache_dir.clone(),
        platform: zvs_host::runtimes::Platform::current(),
        logger: logger.clone(),
    });
    downloads.attach_installer(Arc::new(voice.clone()));
    catalogue.add_provider(voice.catalogue_provider());
    registry.attach_local_engine(Arc::new(runtimes.clone()));
    vector_stores.attach_rerankers(local_rerankers(Arc::new(runtimes.clone())));
    let documents = DocumentExtractorOptions {
        jobs: Arc::new(sidecar.clone()),
        ocr: Some(Arc::new(runtimes.clone())),
        scratch_dir: paths.documents_cache_dir.clone(),
        logger: logger.clone(),
    };
    indexing
        .registry()
        .register(&[".pdf"], Arc::new(PdfExtractor::new(documents.clone())))
        .register(
            &image_extensions(),
            Arc::new(ImageExtractor::new(documents)),
        );
    let nodes = Arc::new(NodeRegistry::new());
    register_core_nodes(&nodes)?;
    register_job_nodes(&nodes)?;
    register_download_nodes(&nodes)?;
    let tools = ToolRegistry::new(ToolRegistryOptions {
        logger: logger.clone(),
        clock: clock.clone(),
        ..ToolRegistryOptions::new(Arc::clone(&database))
    });
    let scenario_effects = EffectsSlot::default();
    register_scenario_nodes(
        &nodes,
        &ScenarioNodeOptions {
            effects: scenario_effects.clone(),
            tools: Some(tools.clone()),
            memory: Some(Arc::new(zvs_host::services::MemoryService::new(
                Arc::clone(&database),
                clock.clone(),
            ))),
        },
    )?;
    let skill_library = SkillLibrary::new(SkillLibraryOptions {
        logger: logger.clone(),
        clock: clock.clone(),
        stored: paths.stored_paths(),
        ..SkillLibraryOptions::new(Arc::clone(&database), paths.skills_dir.clone())
    });
    let artifacts = ArtifactStore::new(paths.conversations_dir.clone())
        .with_library(DASHBOARDS, paths.dashboards_dir.clone())
        .with_dashboards(Arc::new(ProcessDashboardRunner::new(
            Arc::new(toolchain.clone()),
            skill_library.clone(),
        )))
        .with_picker(Arc::clone(&picker))
        .with_launcher(platform::file_launcher())
        .with_thumbnails(paths.thumbnails_dir.clone(), paths.capture_dir.clone())
        .with_pages(Arc::new(RoutedPages::new(
            Arc::new(WorkspacePages(workspace.clone())),
            zvs_host::platform::external::system_url_opener(),
            browser_settings.clone(),
        )));
    let sites = zvs_host::sites::SiteService::new(zvs_host::sites::SiteServiceOptions {
        cache_dir: Some(paths.favicons_dir.clone()),
        pages: Some(Arc::new(RoutedPages::new(
            Arc::new(WorkspacePages(workspace.clone())),
            zvs_host::platform::external::system_url_opener(),
            browser_settings.clone(),
        ))),
        ..zvs_host::sites::SiteServiceOptions::default()
    });
    let roots = VirtualRoots::new(Some(artifacts.clone()), Some(skill_library.clone()));
    register_skill_nodes(&nodes, &skill_library)?;
    register_kernel_tools(&nodes, &tools)?;
    if let Err(error) = skill_library.install_bundled() {
        logger.log(
            LogLevel::Warn,
            "skills",
            "Could not install the bundled skills",
            &[("error", json!(error.message))],
        );
    }
    let mcp = McpService::new(McpServiceOptions {
        logger: logger.clone(),
        clock: clock.clone(),
        client_version: app.package_info().version.to_string(),
        toolchain_dir: Some(ToolchainDirs::new(
            paths.toolchain_dir.clone(),
            paths.package_cache_dir.clone(),
        )),
        toolchain: Some(Arc::new(toolchain.clone())),
        runtime: Some(runtime.clone()),
        opener: Some(zvs_host::platform::external::system_url_opener()),
        ..McpServiceOptions::new(
            Arc::clone(&database),
            tools.clone(),
            Arc::clone(&nodes),
            Arc::clone(&secrets),
            mcp_http,
        )
    });
    let exec_policy = ExecPolicyService::new(ExecPolicyOptions {
        logger: logger.clone(),
        packs: Some(paths.policy_dir.clone()),
        keys: Some(Arc::new(SealedPackKeys::new(
            SettingService::new(Arc::clone(&database), clock.clone()),
            Arc::clone(&secrets),
        ))),
        ..ExecPolicyOptions::new(Arc::clone(&database), clock.clone())
    });
    let runs = RunService::new(RunServiceOptions {
        services: KernelServices {
            providers: Some(registry.clone()),
            vector_stores: Some(Arc::new(vector_stores.clone())),
            jobs: Some(Arc::new(sidecar.clone())),
            downloads: Some(Arc::new(downloads.clone())),
            indexing: Some(Arc::new(indexing.clone())),
            graph: Some(Arc::new(graph.clone())),
            policy: Some(Arc::new(exec_policy.clone())),
        },
        clock: clock.clone(),
        logger: logger.clone(),
        runtime: Some(runtime.clone()),
        tools: Some(tools.clone()),
        ..RunServiceOptions::new(Arc::clone(&database), events.clone(), Arc::clone(&nodes))
    })?;
    let skills = SkillService::new(SkillServiceOptions {
        drivers: Some(registry.clone() as Arc<dyn DriverSource>),
        settings: Some(settings.clone()),
        picker: Some(Arc::clone(&picker)),
        logger: logger.clone(),
        ..SkillServiceOptions::new(skill_library, Arc::clone(&database))
    });
    let scenarios = ScenarioService::new(ScenarioServiceOptions {
        tools: Some(tools.clone()),
        clock: clock.clone(),
        ..ScenarioServiceOptions::new(Arc::clone(&database), Arc::clone(&nodes))
    });
    let forbidden = ForbiddenFolders::from_environment(&[
        paths.root.clone(),
        paths.data_dir.clone(),
        paths.assets_dir.clone(),
        paths.profiles_dir.clone(),
        paths.cache_dir.clone(),
        paths.logs_dir.clone(),
    ]);
    let folders = FolderAccessService::new(
        Arc::clone(&database),
        runs.approvals(),
        forbidden.clone(),
        clock.clone(),
    );
    let documents = DocumentService::new(DocumentServiceOptions {
        forbidden: Some(forbidden),
        artifacts: Some(artifacts.clone()),
        picker: Some(Arc::clone(&picker)),
        clock: clock.clone(),
        logger: logger.clone(),
        ..DocumentServiceOptions::new(
            Arc::clone(&database),
            core.clone(),
            paths.documents_cache_dir.clone(),
        )
    });
    register_file_nodes(
        &nodes,
        &folders,
        FileToolOptions {
            roots: roots.clone(),
            ..FileToolOptions::new(paths.conversations_dir.clone())
        },
    )?;
    let shells = BackgroundShells::new(BackgroundShellOptions::new(events.clone(), clock.clone()));
    register_background_nodes(&nodes, &shells)?;
    register_command_nodes(
        &nodes,
        &folders,
        CommandToolOptions {
            roots,
            shells: Some(shells.clone()),
            ..CommandToolOptions::new(Some(Arc::new(toolchain.clone())), paths.runs_dir.clone())
        },
    )?;
    let grants = BrowserGrants::new(Arc::clone(&database), clock.clone());
    agent.attach(AgentControls {
        runs: Arc::new(runs.clone()),
        grants: grants.clone(),
        limit: setting_limit(settings.clone()),
    });
    register_browser_nodes(
        &nodes,
        &BrowserTools::new(Arc::new(workspace.clone()), grants.clone(), agent.clone()).with_images(
            zvs_host::images::ImageLoader::new(artifacts.clone()),
            clock.clone(),
        ),
    )?;
    browser_settings.subscribe(tool_switch(tools.clone()));
    if !browser_settings.enabled() {
        skills.sync_feature("browser-use", false)?;
    }
    if !zvs_host::computer::settings::command_enabled(&database, "computer-use")? {
        skills.sync_feature("computer-use", false)?;
    }
    let browser_downloads = BrowserDownloads::new(Arc::clone(&database), clock.clone())
        .with_launcher(platform::file_launcher());
    workspace.attach_downloads(browser_settings.clone(), browser_downloads.clone());
    let browser_library = BrowserLibrary::new(Arc::clone(&database), clock.clone());
    workspace.attach_library(browser_library.clone(), sites.clone());
    // Resolve the user's Windows folders independently of the launcher's environment.
    let import_fallback = ImportRoots::system();
    let browser_importer = BrowserImporter::new(
        ImportRoots {
            local: app.path().local_data_dir().ok().or(import_fallback.local),
            roaming: app.path().data_dir().ok().or(import_fallback.roaming),
        },
        browser_library.clone(),
    );
    let browser_extensions = BrowserExtensions::new(&paths.extensions_dir);
    workspace.set_extensions(browser_extensions.enabled_folders());
    {
        let workspace = workspace.clone();
        browser_extensions.on_change(Arc::new(move |folders| workspace.set_extensions(folders)));
    }
    {
        let workspace = workspace.clone();
        browser_settings.subscribe(Arc::new(move |current| {
            workspace.set_full_url(current.full_url);
            workspace.set_autofill(current.password_autosave, current.form_autofill);
            if !current.enabled {
                workspace.hide();
            }
        }));
    }
    let computer_agent = {
        let toolchain = toolchain.clone();
        ComputerAgent::new(
            Arc::new(UvSidecar::new(
                Arc::new(move |args: &[String]| {
                    toolchain.launch(zvs_host::toolchain::ToolchainCommand::Uv, args)
                }),
                paths.computer_dir.clone(),
                logger.clone(),
            )),
            Arc::clone(&database),
            clock.clone(),
            logger.clone(),
            runs.grants(),
        )
    };
    register_computer_nodes(
        &nodes,
        &Computer::new(
            computer_agent.clone(),
            Some(zvs_host::images::ImageLoader::new(artifacts.clone())),
            clock.clone(),
            vec![std::process::id()],
        ),
    )?;
    register_kernel_tools(&nodes, &tools)?;
    let agentic = AgenticService::new(AgenticServiceOptions {
        images: Some(zvs_host::images::ImageLoader::new(artifacts.clone())),
        clock: clock.clone(),
        ..AgenticServiceOptions::new(
            Arc::clone(&database),
            runs.clone(),
            folders.clone(),
            tools.clone(),
        )
    });
    register_agentic_nodes(&nodes, agentic.store(), tools.clone())?;
    let subagents = SubagentService::new(Arc::clone(&database), clock.clone())
        .with_drivers(registry.clone())
        .with_logger(logger.clone());
    let assistant = zvs_host::services::AssistantService::new(Arc::clone(&database), clock.clone())
        .with_logger(logger.clone());
    let workers = WorkerSupervisor::new(clock.clone(), logger.clone());
    let integrations = IntegrationService::new(IntegrationServiceOptions {
        clock: clock.clone(),
        logger: logger.clone(),
        providers: vec![
            Arc::new(MailProvider::new(http.client().clone()).with_clock(clock.clone())),
            Arc::new(TelegramProvider::new(http.client().clone())),
            Arc::new(GitHostingProvider::github(http.client().clone()).with_clock(clock.clone())),
            Arc::new(GitHostingProvider::gitlab(http.client().clone()).with_clock(clock.clone())),
        ],
        opener: Some(workspace_url_opener(&workspace, browser_settings.clone())),
        events: Some(events.clone()),
        ..IntegrationServiceOptions::new(
            Arc::clone(&database),
            tools.clone(),
            Arc::clone(&nodes),
            Arc::clone(&secrets),
            workers.clone(),
        )
    })?;
    let notes = zvs_host::notes::Notes::new(
        artifacts.folders().clone(),
        clock.clone(),
        zvs_host::platform::ids::system_ids(),
    );
    let chat = ChatService::new(ChatServiceOptions {
        notes: Some(notes.clone()),
        browser: Some(browser_settings.clone()),
        images: Some(zvs_host::images::ImageLoader::new(artifacts.clone())),
        tools: Some(tools.clone()),
        skills: Some(Arc::new(skills.clone())),
        mcp: Some(Arc::new(mcp.clone())),
        plugins: Some(Arc::new(integrations.clone())),
        folders: Some(folders.clone()),
        shells: Some(shells.clone()),
        subagents: Some(subagents.clone()),
        assistant: Some(assistant.clone()),
        logger: logger.clone(),
        clock: clock.clone(),
        ..ChatServiceOptions::new(Arc::clone(&database), runs.clone(), Arc::clone(&nodes))
    })?;
    computer_agent.attach(ComputerControls {
        runs: Arc::new(runs.clone()),
        chat: Arc::new(computer::ChatSender(chat.clone())),
        folders: Some(Arc::new(folders.clone())),
    });
    register_kernel_tools(&nodes, &tools)?;
    for key in zvs_host::computer::COMPUTER_TOOLS {
        tools.set_available(
            key,
            zvs_host::computer::settings::command_enabled(&database, "computer-use")?,
        )?;
    }

    let explorer = ExplorerService::new(folders.clone(), events.clone());
    let terminals = TerminalService::new(TerminalServiceOptions {
        folders: folders.clone(),
        events: events.clone(),
        clock: clock.clone(),
        spawner: Arc::new(NativePty),
        shells: Arc::new(SystemShells::from_env()),
        launcher: Some(Arc::new(toolchain.clone())),
        home: std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(std::path::PathBuf::from)
            .unwrap_or_default(),
    });
    downloads.attach(Arc::new(runs.clone()));
    let retention = RetentionService::new(RetentionServiceOptions {
        settings: Some(settings.clone()),
        logger: logger.clone(),
        clock: clock.clone(),
        runtime: Some(runtime.clone()),
        conversations: Some(artifacts.folders().clone()),
        ..RetentionServiceOptions::new(Arc::clone(&database))
    });
    let system = SystemService::new(clock.clone(), events.clone(), DEMO_INTERVAL)
        .with_core(core)
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
        let retention = retention.clone();
        shutdown.register(ShutdownStage::Stop, "retention.stop", move || async move {
            retention.stop();
        });
    }
    {
        let downloads = downloads.clone();
        shutdown.register(ShutdownStage::Stop, "downloads", move || async move {
            downloads.dispose();
        });
    }
    {
        let explorer = explorer.clone();
        shutdown.register(ShutdownStage::Stop, "explorer", move || async move {
            explorer.dispose();
        });
    }
    {
        let terminals = terminals.clone();
        shutdown.register(ShutdownStage::Stop, "terminals", move || async move {
            terminals.dispose();
        });
    }
    {
        let shells = shells.clone();
        shutdown.register(ShutdownStage::Stop, "shells", move || async move {
            shells.dispose().await;
        });
    }
    {
        let runtimes = runtimes.clone();
        shutdown.register(ShutdownStage::Stop, "runtimes", move || async move {
            runtimes.dispose().await;
        });
    }
    {
        let runs = runs.clone();
        shutdown.register(ShutdownStage::Drain, "runs", move || async move {
            runs.dispose().await;
        });
    }
    {
        let providers = providers.clone();
        shutdown.register(ShutdownStage::Dispose, "providers", move || async move {
            providers.dispose().await;
        });
    }
    {
        let mcp = mcp.clone();
        shutdown.register(ShutdownStage::Dispose, "mcp", move || async move {
            mcp.dispose().await;
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

    let notifications = NotificationService::new(NotificationServiceOptions {
        logger: logger.clone(),
        ..NotificationServiceOptions::new(
            Arc::clone(&database),
            settings.clone(),
            Arc::clone(&shell),
            events.clone(),
            clock.clone(),
        )
    });
    runs.watch(notifications.watcher());
    let scenario_runs = ScenarioRunner::new(ScenarioRunnerOptions {
        logger: logger.clone(),
        ..ScenarioRunnerOptions::new(scenarios.clone(), runs.clone())
    });
    chat.on_message(scenario_runs.chat_listener());
    integrations.attach_sink(scenario_runs.integration_sink());
    runs.watch(integrations.run_watcher());
    {
        let integrations = integrations.clone();
        shutdown.register(ShutdownStage::Stop, "integrations", move || async move {
            integrations.dispose().await;
        });
    }
    {
        let workers = workers.clone();
        shutdown.register(ShutdownStage::Stop, "workers", move || async move {
            workers.dispose().await;
        });
    }
    scenario_effects.attach(Arc::new(HostScenarioEffects {
        chat: chat.clone(),
        notifications: Some(notifications.clone()),
        files_dir: paths.scenarios_dir.clone(),
    }));
    notifications.start()?;
    let launched = cold_launch_target(&logger);
    if let Some(target) = launched {
        notifications.activated(target);
    }
    let hidden = launched.is_none()
        && (shell::launched_by_autostart() || cli::launched_by_cli())
        && shell_state.tray();

    let storage = StorageService::new(
        StorageRoots::resolve(&paths),
        Arc::clone(&database),
        Arc::clone(&clock),
    );
    let cli_connections =
        zvs_host::cli::CliConnections::new(Arc::clone(&clock), events.clone(), Arc::clone(&shell));
    let services = Arc::new(Services {
        system,
        storage,
        profile_avatar: zvs_host::services::profile::avatar::ProfileAvatarService::new(
            paths.cache_dir.clone(),
        ),
        data_transfer,
        accounts,
        settings,
        secrets,
        providers,
        health,
        theme,
        shell,
        notifications,
        vector_stores,
        vector_sources,
        vector_eval,
        indexing,
        graph,
        runs,
        downloads,
        runtimes,
        toolchain,
        tools,
        mcp,
        integrations,
        workers,
        skills,
        scenarios,
        scenario_runs,
        memory: chat.memory(),
        notes: notes.clone(),
        agentic,
        chat,
        explorer: explorer.clone(),
        terminals,
        shells,
        documents,
        artifacts,
        sites,
        exec_policy,
        subagents,
        assistant,
        browser: agent.clone(),
        browser_settings,
        browser_sites: grants,
        browser_downloads,
        browser_library,
        browser_importer,
        browser_extensions,
        browser_profile: Some(Arc::new(WorkspaceProfile(workspace.clone()))),
        computer: computer_agent.clone(),
        folders,
        picker: Some(picker),
        cli: platform::cli_installer(paths.cli_path.clone()),
        cli_theme: zvs_host::cli::CliThemeFile::new(&paths.cli_settings_dir),
        cli_preferences: zvs_host::cli::CliPreferencesFile::new(&paths.cli_settings_dir),
        cli_connections,
        voice,
    });
    {
        let services = Arc::clone(&services);
        let app = app.clone();
        let _ = activation.set(Arc::new(move |target| {
            window::focus_studio(&app);
            services.notifications.activated(target);
        }));
    }
    let mut server = IpcServer::new(Arc::clone(&services), logger.clone());
    ipc::register(&mut server);
    server.finish();
    let server = Arc::new(server);
    if e2e::data_dir().is_none() {
        cli::listen(
            Arc::clone(&server),
            events.clone(),
            notices.clone(),
            paths.cli_path.clone(),
            logger.clone(),
        );
    }
    services.accounts.start_auto_refresh()?;
    warm_account_pages(&services.accounts, Arc::clone(&gateway), logger.clone());
    services.health.start();
    retention.start();
    services.runs.recover()?;
    services.chat.recover_goals()?;
    services.downloads.recover()?;
    {
        let mcp = services.mcp.clone();
        tauri::async_runtime::spawn(async move {
            let _ = mcp.start_auto_start().await;
        });
    }
    {
        let integrations = services.integrations.clone();
        let logger = logger.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = integrations.start_enabled().await {
                logger.log(
                    LogLevel::Warn,
                    "integrations",
                    "Could not start the integration accounts",
                    &[("error", json!(error.message))],
                );
            }
        });
    }
    warm_sidecar(sidecar, logger.clone());

    Ok(Launch::Studio {
        host: Host {
            server,
            events: events.clone(),
            services,
        },
        window,
        hidden,
        shell: shell_state,
        logger: logger.clone(),
        lifecycle: Lifecycle {
            restart: std::sync::atomic::AtomicBool::new(false),
            shutdown,
            logger,
            events,
        },
        browser: Browser {
            computer: {
                let appearance = workspace.clone();
                computer::ComputerShell::new(
                    app.clone(),
                    computer_agent,
                    Arc::new(move || appearance.appearance()),
                    notices,
                )
            },
            workspace,
            gateway,
            events: browser_events,
        },
    })
}
