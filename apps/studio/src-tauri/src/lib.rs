mod commands;
mod window;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::focus_studio(app);
        }))
        .manage(commands::EventChannel::default())
        .invoke_handler(tauri::generate_handler![
            commands::ipc_call,
            commands::events_subscribe
        ])
        .setup(|app| {
            window::create_studio(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ZVS AI Studio failed to start");
}
