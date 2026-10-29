use std::sync::Arc;

use zvs_contract::validate::invalid;
use zvs_contract::{SettingsGetInput, SettingsGetOutput, SettingsSetInput};

use super::IpcServer;
use crate::platform::theme::{
    FONT_SIZE_SETTING_KEY, THEME_SETTING_KEY, apply_interface_zoom, apply_theme_source,
};
use crate::policy::EXEC_POLICY_KEY;
use crate::services::Services;
use crate::services::health_check::is_health_check_key;
use crate::services::profile::check_setting;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "settings.get",
            |services: Arc<Services>, input: SettingsGetInput| async move {
                Ok(services.settings.get(&input.key)?.map_or(
                    SettingsGetOutput {
                        key: input.key,
                        value: None,
                        updated_at: None,
                    },
                    SettingsGetOutput::from,
                ))
            },
        )
        .handle(
            "settings.set",
            |services: Arc<Services>, input: SettingsSetInput| async move {
                if input.key == EXEC_POLICY_KEY {
                    return Err(invalid("key", "reserved"));
                }
                check_setting(&input.key, &input.value)?;
                services.notifications.apply(&input.key, &input.value)?;
                let result = services.settings.set(&input.key, &input.value)?;
                if is_health_check_key(&input.key) {
                    services.health.stop();
                    services.health.start();
                }
                if input.key == THEME_SETTING_KEY {
                    apply_theme_source(Some(&input.value), services.theme.as_ref());
                }
                if input.key == FONT_SIZE_SETTING_KEY {
                    apply_interface_zoom(Some(&input.value), services.theme.as_ref());
                }
                Ok(result)
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::platform::theme::ThemeSource;
    use crate::services::SystemService;
    use crate::testing::{TestServices, test_services};

    fn server() -> (TestServices, IpcServer) {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        (host, server)
    }

    #[tokio::test]
    async fn settings_channels_carry_json_values_through_the_whole_chain() {
        let (_host, server) = server();
        let geometry = json!({ "bounds": { "x": 10, "y": 20, "width": 1600, "height": 1000 }, "maximized": false });

        assert_eq!(
            server
                .dispatch("settings.get", json!({ "key": "window.main" }))
                .await,
            json!({ "ok": true, "data": { "key": "window.main" } })
        );

        let written = server
            .dispatch(
                "settings.set",
                json!({ "key": "window.main", "value": geometry }),
            )
            .await;
        assert_eq!(
            written,
            json!({ "ok": true, "data": { "key": "window.main", "value": geometry, "updatedAt": 1_700_000_000_000_i64 } })
        );
        assert_eq!(
            server
                .dispatch("settings.get", json!({ "key": "window.main" }))
                .await,
            written
        );
    }

    #[tokio::test]
    async fn saving_the_theme_setting_mirrors_it_to_the_platform_other_keys_do_not() {
        let (host, server) = server();
        for (key, value) in [
            ("ui.theme", json!("light")),
            ("ui.rail.collapsed", json!(true)),
            ("ui.theme", json!("sepia")),
        ] {
            let response = server
                .dispatch("settings.set", json!({ "key": key, "value": value }))
                .await;
            assert_eq!(response["ok"], json!(true));
        }
        assert_eq!(host.theme.applied(), [ThemeSource::Light]);
    }

    #[tokio::test]
    async fn saving_a_health_check_setting_restarts_the_background_check() {
        let (host, server) = server();
        assert!(!host.health.running());
        for (key, value, running) in [
            ("providers.healthcheck.minutes", json!(2), false),
            ("providers.healthcheck.enabled", json!(true), true),
            ("providers.healthcheck.minutes", json!(5), true),
            ("ui.rail.collapsed", json!(true), true),
            ("providers.healthcheck.enabled", json!(false), false),
        ] {
            let response = server
                .dispatch("settings.set", json!({ "key": key, "value": value }))
                .await;
            assert_eq!(response["ok"], json!(true));
            assert_eq!(host.health.running(), running, "{key}");
        }
        assert_eq!(host.health.interval_minutes(), 5);
    }

    #[tokio::test]
    async fn the_tray_and_autostart_settings_reach_the_shell_before_they_are_stored() {
        let (host, server) = server();
        for (key, value) in [
            ("app.tray", json!(true)),
            ("app.autostart", json!(true)),
            ("app.tray", json!(false)),
            ("ui.rail.collapsed", json!(true)),
        ] {
            let response = server
                .dispatch("settings.set", json!({ "key": key, "value": value }))
                .await;
            assert_eq!(response["ok"], json!(true), "{key}");
        }
        assert_eq!(host.shell.tray(), [true, false]);
        assert_eq!(host.shell.registered(), Some(true));
    }

    #[tokio::test]
    async fn a_refused_autostart_change_fails_and_keeps_the_stored_value() {
        let (host, server) = server();
        host.settings
            .set("app.autostart", &json!(false))
            .expect("stores");
        host.shell.fail_autostart(true);
        let response = server
            .dispatch(
                "settings.set",
                json!({ "key": "app.autostart", "value": true }),
            )
            .await;
        assert_eq!(response["error"]["code"], json!("NATIVE_ERROR"));
        assert_eq!(
            response["error"]["message"],
            json!(crate::notifications::copy::AUTOSTART_FAILED)
        );
        assert_eq!(
            host.settings
                .get("app.autostart")
                .expect("reads")
                .map(|setting| setting.value),
            Some(json!(false))
        );
    }

    #[tokio::test]
    async fn a_malformed_key_is_rejected_before_it_reaches_the_database() {
        let (host, server) = server();
        let response = server
            .dispatch("settings.set", json!({ "key": "UI theme", "value": 1 }))
            .await;
        assert_eq!(response["error"]["code"], json!("VALIDATION_FAILED"));
        assert!(host.settings.list().expect("lists").is_empty());
    }

    #[tokio::test]
    async fn an_out_of_range_profile_value_is_rejected_and_not_stored() {
        let (host, server) = server();
        for (key, value) in [
            ("chat.toolcalls", json!(0)),
            ("profile.style", json!("loud")),
        ] {
            let response = server
                .dispatch("settings.set", json!({ "key": key, "value": value }))
                .await;
            assert_eq!(
                response["error"]["code"],
                json!("VALIDATION_FAILED"),
                "{key}"
            );
        }
        assert!(host.settings.list().expect("lists").is_empty());
        let response = server
            .dispatch(
                "settings.set",
                json!({ "key": "chat.toolcalls", "value": 20 }),
            )
            .await;
        assert_eq!(response["ok"], json!(true));
    }
}
