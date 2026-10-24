pub mod browser;
mod commands;
mod e2e;
mod picker;
mod platform;
mod shell;
mod theme;
mod toast;
mod window;

use std::error::Error;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde_json::json;
use tauri::{App, AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use zvs_contract::{NOTIFICATION_SCHEME, NotificationTargetDto};
use zvs_host::artifacts::ArtifactStore;
use zvs_host::browser::{BrowserLifecycle, profile_dir};
use zvs_host::commands::{CommandToolOptions, register_command_nodes};
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
use zvs_host::drivers::mcp::TOOLCHAIN_DIR;
use zvs_host::drivers::sidecar::{SidecarDriver, SidecarOptions};
use zvs_host::files::{FileToolOptions, register_file_nodes};
use zvs_host::indexing::{
    DocumentExtractorOptions, ImageExtractor, IndexingService, IndexingServiceOptions,
    PdfExtractor, image_extensions,
};
use zvs_host::kernel::{
    KernelServices, NodeRegistry, register_core_nodes, register_download_nodes, register_job_nodes,
};
use zvs_host::notifications::{NotificationService, NotificationServiceOptions};
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
use zvs_host::platform::shell::SharedShellPort;
use zvs_host::platform::shutdown::{
    DEFAULT_SHUTDOWN_CAP, ExitDecision, ShutdownCoordinator, ShutdownStage,
};
use zvs_host::platform::theme::{
    FONT_SIZE_SETTING_KEY, SharedThemePort, THEME_SETTING_KEY, apply_interface_zoom,
    apply_theme_source,
};
use zvs_host::platform::window_state::{WINDOW_STATE_KEY, WindowState, read_window_state};
use zvs_host::runtimes::{
    ReleaseResolver, ReleaseResolverOptions, RuntimeService, RuntimeServiceOptions,
};
use zvs_host::scenarios::{
    EffectsSlot, HostScenarioEffects, SCENARIO_FILES_DIR, ScenarioNodeOptions, ScenarioRunner,
    ScenarioRunnerOptions, ScenarioService, ScenarioServiceOptions, register_scenario_nodes,
};
use zvs_host::services::rerank::local_rerankers;
use zvs_host::services::system::DEMO_INTERVAL;
use zvs_host::services::{
    AccountService, AccountServiceOptions, ChatService, ChatServiceOptions, CryptoService,
    DriverSource, GraphService, GraphServiceOptions, HealthCheckOptions, HealthCheckService,
    McpService, McpServiceOptions, ProviderService, ProviderServiceOptions, RetentionService,
    RetentionServiceOptions, RunService, RunServiceOptions, SecretService, Services,
    SettingService, SystemService, VectorEvalService, VectorEvalServiceOptions,
    VectorSourceService, VectorSourceServiceOptions, VectorStoreService, VectorStoreServiceOptions,
};
use zvs_host::skills::{
    BUNDLED_SKILLS, SKILLS_DIR, SkillLibrary, SkillLibraryOptions, SkillScriptOptions,
    SkillService, SkillServiceOptions, register_skill_nodes,
};
use zvs_host::terminal::{NativePty, SystemShells, TerminalService, TerminalServiceOptions};
use zvs_host::toolchain::{
    ToolchainResolver, ToolchainResolverOptions, ToolchainService, ToolchainServiceOptions,
};
use zvs_host::tools::{ToolInvoker, ToolRegistry, ToolRegistryOptions, register_kernel_tools};
use zvs_host::workspace::{ExplorerService, FolderAccessService, ForbiddenFolders};
use zvs_host::{IpcServer, ipc};

use crate::browser::gateway::GatewayOptions;
use crate::browser::{
    AccountGateway, BearerTokens, BrowserWorkspace, WebviewPages, WorkspaceOpener, gateway_sessions,
};
use crate::commands::Host;
use crate::picker::DialogPicker;
use crate::shell::{ShellState, TauriShell};
use crate::theme::TauriTheme;
use zvs_host::platform::picker::SharedPicker;

enum Boot {
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
}

struct Lifecycle {
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
            commands::browser_subscribe
        ])
        .setup(|app| {
            match compose(app)? {
                Boot::Studio {
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
                    app.manage(browser.workspace);
                    app.manage(Arc::clone(&gateway));
                    app.manage(browser.events);
                    app.manage(host);
                    app.manage(lifecycle);
                    window::create_studio(app.handle(), window, hidden, shell, move |state| {
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
                    if let Ok(stored) = zoom_services.settings.get(FONT_SIZE_SETTING_KEY) {
                        apply_interface_zoom(
                            stored.map(|setting| setting.value).as_ref(),
                            zoom_services.theme.as_ref(),
                        );
                    }
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

fn register_link_scheme(app: &App, logger: &SharedLogger) {
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

const APP_NAME: &str = "ZVS AI Studio";
const TOAST_ICON: &[u8] = include_bytes!("../icons/128x128.png");

fn write_toast_icon(data_dir: &Path) -> Option<std::path::PathBuf> {
    let icon = data_dir.join("shell").join("notification.png");
    if std::fs::read(&icon).ok().as_deref() != Some(TOAST_ICON) {
        std::fs::create_dir_all(icon.parent()?).ok()?;
        std::fs::write(&icon, TOAST_ICON).ok()?;
    }
    Some(icon)
}

fn compose(app: &App) -> Result<Boot, Box<dyn Error>> {
    let data_dir = match e2e::data_dir() {
        Some(dir) => dir,
        None => app.path().app_data_dir()?,
    };
    let paths = StudioPaths::resolve(&PathEnvironment {
        data_dir: data_dir.clone(),
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
    let shell_state = ShellState::shared();
    let activation: toast::ActivationSlot = Arc::new(OnceLock::new());
    app.manage(Arc::clone(&activation));
    if !paths.packaged && e2e::data_dir().is_none() {
        register_link_scheme(app, &logger);
    }
    let toast_icon = write_toast_icon(&paths.data_dir);
    let toaster = platform::toaster(
        &app.config().identifier,
        APP_NAME,
        toast_icon.as_deref(),
        Arc::clone(&activation),
        logger.clone(),
    );
    let shell: SharedShellPort = Arc::new(TauriShell::new(
        app.handle().clone(),
        Arc::clone(&shell_state),
        toaster,
    ));
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
    let profile = profile_dir(&data_dir);
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

    let http = HttpClient::new()?;
    let mcp_http = http.client().clone();
    let mut registry = ProviderRegistryOptions::new(
        Arc::new(DatabaseProviders::new(Arc::clone(&database))),
        secrets.clone(),
        http.clone(),
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
        runtime: Some(runtime.clone()),
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
        paths.downloads_dir.clone(),
        paths.vector_stores_dir.clone(),
        paths.data_dir.clone(),
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
        ..DownloadServiceOptions::new(
            Arc::clone(&database),
            Arc::new(sidecar.clone()),
            disk,
            catalogue.clone(),
            paths.downloads_dir.clone(),
            logger.clone(),
            clock.clone(),
        )
    });
    let runtimes = RuntimeService::new(RuntimeServiceOptions {
        downloads: Arc::new(downloads.clone()),
        catalogue: Arc::new(catalogue.clone()),
        settings: settings.clone(),
        downloads_dir: paths.downloads_dir.clone(),
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
    let picker: SharedPicker = Arc::new(DialogPicker::new(app.handle().clone()));
    let toolchain = ToolchainService::new(ToolchainServiceOptions {
        picker: Some(Arc::clone(&picker)),
        ..ToolchainServiceOptions::new(
            paths.data_dir.join(TOOLCHAIN_DIR),
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
    registry.attach_local_engine(Arc::new(runtimes.clone()));
    vector_stores.attach_rerankers(local_rerankers(Arc::new(runtimes.clone())));
    let documents = DocumentExtractorOptions {
        jobs: Arc::new(sidecar.clone()),
        ocr: Some(Arc::new(runtimes.clone())),
        scratch_dir: paths.cache_dir.join("documents"),
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
        settings: Some(settings.clone()),
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
        },
    )?;
    let skill_library = SkillLibrary::new(SkillLibraryOptions {
        logger: logger.clone(),
        clock: clock.clone(),
        ..SkillLibraryOptions::new(
            Arc::clone(&database),
            paths.data_dir.join(SKILLS_DIR),
            tools.clone(),
        )
    });
    let artifacts = ArtifactStore::new(paths.data_dir.join("artifacts"))
        .with_picker(Arc::clone(&picker))
        .with_launcher(platform::file_launcher());
    register_skill_nodes(
        &nodes,
        &skill_library,
        SkillScriptOptions {
            artifacts: Some(artifacts.clone()),
            ..SkillScriptOptions::new(
                Some(Arc::new(toolchain.clone())),
                paths.cache_dir.join("skill-scripts"),
            )
        },
    )?;
    register_kernel_tools(&nodes, &tools)?;
    if let Err(error) = skill_library.install_bundled(BUNDLED_SKILLS) {
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
        toolchain_dir: Some(paths.data_dir.join(TOOLCHAIN_DIR)),
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
    let runs = RunService::new(RunServiceOptions {
        services: KernelServices {
            providers: Some(registry.clone()),
            vector_stores: Some(Arc::new(vector_stores.clone())),
            jobs: Some(Arc::new(sidecar.clone())),
            downloads: Some(Arc::new(downloads.clone())),
            indexing: Some(Arc::new(indexing.clone())),
            graph: Some(Arc::new(graph.clone())),
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
        ..SkillServiceOptions::new(skill_library, runs.clone(), Arc::clone(&database))
    });
    let scenarios = ScenarioService::new(ScenarioServiceOptions {
        tools: Some(tools.clone()),
        clock: clock.clone(),
        ..ScenarioServiceOptions::new(Arc::clone(&database), Arc::clone(&nodes))
    });
    let forbidden = ForbiddenFolders::from_environment(&[
        paths.data_dir.clone(),
        paths.cache_dir.clone(),
        paths.logs_dir.clone(),
        paths.downloads_dir.clone(),
        profile_dir(&paths.data_dir),
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
            paths.cache_dir.join("documents"),
        )
    });
    register_file_nodes(
        &nodes,
        &folders,
        FileToolOptions::new(paths.runs_dir.clone()),
    )?;
    register_command_nodes(
        &nodes,
        &folders,
        runs.approvals(),
        CommandToolOptions::new(
            Some(Arc::new(toolchain.clone())),
            paths.cache_dir.join("code-runs"),
        ),
    )?;
    register_kernel_tools(&nodes, &tools)?;
    let chat = ChatService::new(ChatServiceOptions {
        tools: Some(tools.clone()),
        skills: Some(Arc::new(skills.clone())),
        folders: Some(folders.clone()),
        logger: logger.clone(),
        clock: clock.clone(),
        ..ChatServiceOptions::new(Arc::clone(&database), runs.clone(), Arc::clone(&nodes))
    })?;
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
    let tool_invoker = ToolInvoker::new(tools.clone(), nodes, runs.clone(), clock.clone());
    downloads.attach(Arc::new(runs.clone()));
    let retention = RetentionService::new(RetentionServiceOptions {
        settings: Some(settings.clone()),
        logger: logger.clone(),
        clock: clock.clone(),
        runtime: Some(runtime.clone()),
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
    scenario_effects.attach(Arc::new(HostScenarioEffects {
        chat: chat.clone(),
        notifications: Some(notifications.clone()),
        files_dir: paths.data_dir.join(SCENARIO_FILES_DIR),
    }));
    notifications.start()?;
    let launched = cold_launch_target(&logger);
    if let Some(target) = launched {
        notifications.activated(target);
    }
    let hidden = launched.is_none() && shell::launched_by_autostart() && shell_state.tray();

    let services = Arc::new(Services {
        system,
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
        tool_invoker,
        mcp,
        skills,
        scenarios,
        scenario_runs,
        memory: chat.memory(),
        chat,
        explorer: explorer.clone(),
        terminals,
        documents,
        artifacts,
        folders,
        picker: Some(picker),
    });
    {
        let services = Arc::clone(&services);
        let app = app.handle().clone();
        let _ = activation.set(Arc::new(move |target| {
            window::focus_studio(&app);
            services.notifications.activated(target);
        }));
    }
    let mut server = IpcServer::new(Arc::clone(&services), logger.clone());
    ipc::register(&mut server);
    server.finish();
    services.accounts.start_auto_refresh()?;
    warm_account_pages(&services.accounts, Arc::clone(&gateway), logger.clone());
    services.health.start();
    retention.start();
    services.runs.recover()?;
    services.downloads.recover()?;
    {
        let mcp = services.mcp.clone();
        tauri::async_runtime::spawn(async move {
            let _ = mcp.start_auto_start().await;
        });
    }
    warm_sidecar(sidecar, logger.clone());

    Ok(Boot::Studio {
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
