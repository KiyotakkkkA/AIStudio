mod commands;
mod theme;
mod window;

use std::error::Error;
use std::sync::Arc;

use serde_json::json;
use tauri::{App, Manager};
use zvs_host::data::{MigrationFailed, PrepareOptions, prepare_database};
use zvs_host::platform::clock::{SharedClock, SystemClock};
use zvs_host::platform::events::{EventBus, EventBusOptions, EventRecorder};
use zvs_host::platform::ids::system_ids;
use zvs_host::platform::logger::{FileLogger, LogLevel, LoggerOptions, SharedLogger};
use zvs_host::platform::paths::{PathEnvironment, StudioPaths};
use zvs_host::platform::recorder::{FileEventRecorder, recording_enabled_from_env};
use zvs_host::platform::theme::{SharedThemePort, THEME_SETTING_KEY, apply_theme_source};
use zvs_host::platform::window_state::{WINDOW_STATE_KEY, WindowState, read_window_state};
use zvs_host::services::system::DEMO_INTERVAL;
use zvs_host::services::{Services, SettingService, SystemService};
use zvs_host::{IpcServer, ipc};

use crate::commands::Host;
use crate::theme::TauriTheme;

enum Boot {
    Studio {
        host: Host,
        window: WindowState,
        logger: SharedLogger,
    },
    Recovery(MigrationFailed),
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::focus_studio(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::ipc_call,
            commands::events_subscribe
        ])
        .setup(|app| {
            match compose(app)? {
                Boot::Studio {
                    host,
                    window,
                    logger,
                } => {
                    let services = Arc::clone(&host.services);
                    app.manage(host);
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
                }
                Boot::Recovery(failure) => window::create_recovery(app.handle(), &failure)?,
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ZVS AI Studio failed to start");
}

fn compose(app: &App) -> Result<Boot, Box<dyn Error>> {
    let paths = StudioPaths::resolve(&PathEnvironment {
        data_dir: app.path().app_data_dir()?,
        resource_dir: app.path().resource_dir()?,
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
    let settings = SettingService::new(database, clock.clone());
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

    let services = Arc::new(Services {
        system: SystemService::new(clock, events.clone(), DEMO_INTERVAL),
        settings,
        theme,
    });
    let mut server = IpcServer::new(Arc::clone(&services), logger.clone());
    ipc::register(&mut server);
    server.finish();

    Ok(Boot::Studio {
        host: Host {
            server,
            events,
            services,
        },
        window,
        logger,
    })
}
