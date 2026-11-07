use std::sync::Arc;

use hand_android_core::{Companion, CompanionConfig, may_load, parse_target, proxy_origin};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

struct AppState {
    companion: Arc<Companion>,
}

#[tauri::command]
fn proxy_url(state: tauri::State<AppState>) -> String {
    state.companion.url("/")
}

#[tauri::command]
fn phone_endpoint_id(state: tauri::State<AppState>) -> String {
    state.companion.endpoint_id().to_string()
}

#[tauri::command]
async fn open_board(
    state: tauri::State<'_, AppState>,
    endpoint_id: String,
    relay: Option<String>,
    fleet: Option<String>,
) -> Result<String, String> {
    let target = parse_target(&endpoint_id, relay.as_deref())?;
    let path = match fleet.map(|fleet| fleet.trim().to_owned()) {
        Some(fleet) if !fleet.is_empty() => {
            if !valid_fleet(&fleet) {
                return Err(
                    "the fleet ID must be f and 12 lowercase hexadecimal characters".to_owned(),
                );
            }
            format!("/{fleet}/pair")
        }
        _ => "/".to_owned(),
    };
    state.companion.set_target(Some(target)).await;
    Ok(state.companion.url(&path))
}

fn valid_fleet(fleet: &str) -> bool {
    fleet.len() == 13
        && fleet.starts_with('f')
        && fleet[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bundled_origin(config: &tauri::Config) -> tauri::Url {
    #[cfg(dev)]
    let configured = config.build.dev_url.clone();
    #[cfg(not(dev))]
    let configured = match config.build.frontend_dist.as_ref() {
        Some(tauri::utils::config::FrontendDist::Url(url)) => Some(url.clone()),
        _ => None,
    };
    configured.unwrap_or_else(|| {
        let scheme = if config
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
            .is_some_and(|window| window.use_https_scheme)
        {
            "https"
        } else {
            "http"
        };
        tauri::Url::parse(&format!("{scheme}://tauri.localhost"))
            .expect("the bundled origin parses")
    })
}

fn origin_guard(bundled: tauri::Url) -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("board-origin")
        .js_init_script(include_str!("external_links.js"))
        .on_navigation(move |webview, url| {
            let proxy = webview
                .try_state::<AppState>()
                .map(|state| proxy_origin(state.companion.port()));
            if may_load(url, &bundled, proxy.as_ref()) {
                return true;
            }
            let _ = webview.opener().open_url(url.as_str(), None::<&str>);
            false
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let bundled = bundled_origin(context.config());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::Builder::new().build())
        .plugin(origin_guard(bundled))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let companion =
                tauri::async_runtime::block_on(Companion::start(CompanionConfig { data_dir }))?;
            app.manage(AppState {
                companion: Arc::new(companion),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            proxy_url,
            phone_endpoint_id,
            open_board
        ])
        .build(context)
        .expect("the Tauri app builds")
        .run(|app, event| {
            if let tauri::RunEvent::Resumed = event {
                let state = app.state::<AppState>();
                let companion = state.companion.clone();
                tauri::async_runtime::spawn(async move {
                    companion.reconnect_on_resume().await;
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_origin_follows_the_app_config() {
        let config = tauri::Config::default();
        assert_eq!(bundled_origin(&config).as_str(), "http://tauri.localhost/");

        let mut https = tauri::Config::default();
        https.app.windows = vec![tauri::utils::config::WindowConfig {
            use_https_scheme: true,
            ..Default::default()
        }];
        assert_eq!(bundled_origin(&https).as_str(), "https://tauri.localhost/");
    }

    #[cfg(not(dev))]
    #[test]
    fn the_bundled_origin_follows_a_frontend_dist_url() {
        let mut config = tauri::Config::default();
        config.build.frontend_dist = Some(tauri::utils::config::FrontendDist::Url(
            tauri::Url::parse("https://app.example.test/").unwrap(),
        ));
        assert_eq!(
            bundled_origin(&config).as_str(),
            "https://app.example.test/"
        );
    }
}
