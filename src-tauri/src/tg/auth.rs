//! Login flow: phone → code → 2FA password.

use crate::state::{Inner, SharedState};
use crate::tg::client::{ensure_client, logged_in_name};
use grammers_client::tl;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SignInOutcome {
    Ok { user: String },
    NeedPassword { hint: Option<String> },
}

pub async fn send_code(st: &Inner, phone: &str) -> Result<(), String> {
    let _auth = st.auth_lock.lock().await;
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
    *st.password_token.lock().await = None;
    Ok(())
}

pub async fn sign_in(st: &SharedState, code: &str) -> Result<SignInOutcome, String> {
    let _auth = st.auth_lock.lock().await;
    let client = ensure_client(st).await?;
    let mut pending = st.login_token.lock().await;
    let token = pending.as_ref().ok_or("NO_PENDING_CODE")?;
    match client.sign_in(token, code).await {
        Ok(_) => {
            pending.take();
            drop(pending);
            let user = logged_in_name(&client).await.unwrap_or_else(|| "TG".into());
            on_authorized(st).await;
            Ok(SignInOutcome::Ok { user })
        }
        Err(grammers_client::SignInError::PasswordRequired(password_token)) => {
            pending.take();
            // keep the token around for the password step
            let hint = password_token.hint().map(str::to_owned);
            *st.password_token.lock().await = Some(password_token);
            Ok(SignInOutcome::NeedPassword { hint })
        }
        Err(grammers_client::SignInError::InvalidCode) => Err("INVALID_CODE".into()),
        Err(e) => Err(format!("SIGN_IN: {e}")),
    }
}

pub async fn check_password(st: &SharedState, password: &str) -> Result<String, String> {
    let _auth = st.auth_lock.lock().await;
    let client = ensure_client(st).await?;
    // A failed refresh can be retried in place without sending another SMS.
    let token = match st.password_token.lock().await.take() {
        Some(token) => token,
        None => fresh_password_token(&client).await?,
    };
    let check = client.check_password(token, password).await;
    if let Err(e) = check {
        let invalid = matches!(&e, grammers_client::SignInError::InvalidPassword(_));
        // Wrong password: rebuild a fresh password token (auth.getPassword)
        // so the user can retry without redoing the code step.
        if let Ok(token) = fresh_password_token(&client).await {
            *st.password_token.lock().await = Some(token);
        } else if let grammers_client::SignInError::InvalidPassword(token) = e {
            *st.password_token.lock().await = Some(token);
            return Err("INVALID_PASSWORD".into());
        }
        if invalid {
            return Err("INVALID_PASSWORD".into());
        }
        return Err(format!("PASSWORD: {e}"));
    }
    let user = logged_in_name(&client).await.unwrap_or_else(|| "TG".into());
    on_authorized(st).await;
    Ok(user)
}

pub async fn logout(st: &Inner) -> Result<(), String> {
    let _auth = st.auth_lock.lock().await;
    crate::tg::client::reset_client(st).await;
    *st.login_token.lock().await = None;
    *st.password_token.lock().await = None;
    match std::fs::remove_file(&st.session_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}

/// Fire-and-forget post-login work: warm the peer cache and start the watcher.
async fn on_authorized(st: &SharedState) {
    st.queue
        .no_auth_until
        .store(0, std::sync::atomic::Ordering::Relaxed);
    st.bus.emit("auth:changed", serde_json::Value::Bool(true));
    // Warm the session peer cache (helps update catch-up and t.me/c resolution).
    let cache_st = st.clone();
    tokio::spawn(async move {
        if let Ok(client) = ensure_client(&cache_st).await {
            let mut dialogs = client.iter_dialogs();
            for _ in 0..100 {
                if dialogs.next().await.is_err() {
                    break;
                }
            }
        }
    });
    crate::tg::watcher::spawn(st.clone());
}

async fn fresh_password_token(
    client: &grammers_client::Client,
) -> Result<grammers_client::client::PasswordToken, String> {
    let tl::enums::account::Password::Password(password) = client
        .invoke(&tl::functions::account::GetPassword {})
        .await
        .map_err(|e| format!("PASSWORD: {e}"))?;
    Ok(grammers_client::client::PasswordToken::new(password))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_outcomes_match_the_frontend_discriminated_union() {
        assert_eq!(
            serde_json::to_value(SignInOutcome::Ok {
                user: "demo".into()
            })
            .unwrap()["status"],
            "ok"
        );
        assert_eq!(
            serde_json::to_value(SignInOutcome::NeedPassword { hint: None }).unwrap()["status"],
            "need_password"
        );
    }
}
