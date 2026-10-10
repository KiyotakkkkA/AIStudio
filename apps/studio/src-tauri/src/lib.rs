mod commands;
mod window;

use std::error::Error;
use std::sync::Arc;

use serde_json::json;
use tauri::{App, Manager};
use zvs_host::platform::clock::{SharedClock, SystemClock};
use zvs_host::platform::events::{EventBus, EventBusOptions, EventRecorder};
use zvs_host::platform::ids::system_ids;
use zvs_host::platform::logger::{FileLogger, LogLevel, LoggerOptions, SharedLogger};
use zvs_host::platform::paths::{PathEnvironment, StudioPaths};
use zvs_host::platform::recorder::{FileEventRecorder, recording_enabled_from_env};
use zvs_host::services::system::DEMO_INTERVAL;
use zvs_host::services::{Services, SystemService};
use zvs_host::{IpcServer, ipc};

use crate::commands::Host;

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
            let host = compose(app)?;
            app.manage(host);
            window::create_studio(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ZVS AI Studio failed to start");
}

fn compose(app: &App) -> Result<Host, Box<dyn Error>> {
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
    });
    let mut server = IpcServer::new(services, logger);
    ipc::register(&mut server);
    server.finish();

    Ok(Host { server, events })
}
