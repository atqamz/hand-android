use std::sync::Arc;

#[cfg(target_os = "android")]
mod keystore;

#[cfg(target_os = "android")]
use hand_android_core::CompanionConfig;
use hand_android_core::{Companion, parse_target};
use tauri::Manager;

struct AppState {
    companion: Result<Arc<Companion>, String>,
}

impl AppState {
    fn companion(&self) -> Result<Arc<Companion>, String> {
        self.companion.clone()
    }
}

#[tauri::command]
fn proxy_url(state: tauri::State<AppState>) -> Result<String, String> {
    Ok(state.companion()?.url("/"))
}

#[tauri::command]
fn phone_endpoint_id(state: tauri::State<AppState>) -> Result<String, String> {
    Ok(state.companion()?.endpoint_id().to_string())
}

#[tauri::command]
async fn open_board(
    state: tauri::State<'_, AppState>,
    endpoint_id: String,
    relay: Option<String>,
    fleet: Option<String>,
) -> Result<String, String> {
    let companion = state.companion()?;
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
    companion.set_target(Some(target)).await;
    Ok(companion.url(&path))
}

fn valid_fleet(fleet: &str) -> bool {
    fleet.len() == 13
        && fleet.starts_with('f')
        && fleet[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(keystore::plugin());
    builder
        .setup(|app| {
            #[cfg(target_os = "android")]
            {
                let data_dir = app.path().app_data_dir()?;
                let cipher = Arc::new(app.state::<keystore::KeystoreCipher>().inner().clone());
                let companion = tauri::async_runtime::block_on(Companion::start(CompanionConfig {
                    data_dir,
                    cipher,
                }))
                .map(Arc::new)
                .map_err(|error| format!("the phone's key could not start the companion: {error}"));
                app.manage(AppState { companion });
            }
            #[cfg(not(target_os = "android"))]
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            proxy_url,
            phone_endpoint_id,
            open_board
        ])
        .build(tauri::generate_context!())
        .expect("the Tauri app builds")
        .run(|app, event| {
            if let tauri::RunEvent::Resumed = event {
                let state = app.state::<AppState>();
                if let Ok(companion) = &state.companion {
                    let companion = companion.clone();
                    tauri::async_runtime::spawn(async move {
                        companion.reconnect_on_resume().await;
                    });
                }
            }
        });
}
