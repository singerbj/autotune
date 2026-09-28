/// Commands the webview may invoke. Tauri generates an `allow-<command>`
/// permission for each; `capabilities/default.json` grants exactly these, so
/// the UI can reach nothing else (Architecture › Shell features).
const COMMANDS: &[&str] = &[
    "ping",
    "get_app_info",
    "list_devices",
    "get_config",
    "set_config",
    "start_engine",
    "stop_engine",
    "get_engine_status",
    "set_params",
    "set_bypass",
    "run_latency_test",
    "run_setup_check",
    "set_route_all_apps",
    "open_asio_panel",
    "get_diagnostics",
    "complete_wizard",
    "set_launch_at_login",
    "save_preset",
    "load_preset",
    "delete_preset",
    "get_update_status",
    "check_for_update",
    "install_update",
    "hide_window",
    "quit_app",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .unwrap_or_else(|e| panic!("tauri build failed: {e}"));
}
