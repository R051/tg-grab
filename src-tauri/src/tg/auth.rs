//! Login flow: phone → code → 2FA password.

use crate::state::{Inner, SharedState};
use crate::tg::client::{ensure_client, logged_in_name};
use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum SignInOutcome {
    Ok { user: String },
    NeedPassword { hint: Option<String> },
}

pub async fn send_code(st: &Inner, phone: &str) -> Result<(), String> {
    let client = ensure_client(st).await?;
    let api_hash = st
        .config
        .read()
        .await
        .api_hash
        .clone()
        .ok_or("API_NOT_CONFIGURED")?;
    let token = client
        .request_login_code(phone, &api_hash)
        .await
        .map_err(|e| format!("SEND_CODE: {e}"))?;
    *st.login_token.lock().await = Some(token);
    Ok(())
}

pub async fn sign_in(st: &SharedState, app: &AppHandle, code: &str) -> Result<SignInOutcome, String> {
    let client = ensure_client(st).await?;
    let token = st
        .login_token
        .lock()
        .await
        .take()
        .ok_or("NO_PENDING_CODE")?;
    match client.sign_in(&token, code).await {
        Ok(_) => {
            let user = logged_in_name(&client).await.unwrap_or_else(|| "TG".into());
            on_authorized(st, app).await;
            Ok(SignInOutcome::Ok { user })
        }
        Err(grammers_client::SignInError::PasswordRequired(password_token)) => {
            // keep the token around for the password step
            let hint = password_token.hint().map(str::to_owned);
            *st.password_token.lock().await = Some(password_token);
            Ok(SignInOutcome::NeedPassword { hint })
        }
        Err(e) => Err(format!("SIGN_IN: {e}")),
    }
}

pub async fn check_password(st: &SharedState, app: &AppHandle, password: &str) -> Result<String, String> {
    let client = ensure_client(st).await?;
    let token = st
        .password_token
        .lock()
        .await
        .take()
        .ok_or("NO_PENDING_PASSWORD")?;
    client
        .check_password(token, password)
        .await
        .map_err(|e| format!("PASSWORD: {e}"))?;
    let user = logged_in_name(&client).await.unwrap_or_else(|| "TG".into());
    on_authorized(st, app).await;
    Ok(user)
}

pub async fn logout(st: &Inner) -> Result<(), String> {
    crate::tg::client::reset_client(st).await;
    *st.login_token.lock().await = None;
    match std::fs::remove_file(&st.session_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}

/// Fire-and-forget post-login work: warm the peer cache and start the watcher.
async fn on_authorized(st: &SharedState, app: &AppHandle) {
    // Warm the session peer cache (helps update catch-up and t.me/c resolution).
    let cache_st = st.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(client) = ensure_client(&cache_st).await {
            let mut dialogs = client.iter_dialogs();
            for _ in 0..100 {
                if dialogs.next().await.is_err() {
                    break;
                }
            }
        }
    });
    crate::tg::watcher::spawn(st.clone(), app.clone());
}
