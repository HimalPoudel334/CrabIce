//! One user-level "send": optional token refresh before, one automatic retry
//! after a 401, with every step reported through a single callback.
//!
//! Matches the iced app's behavior: a Bearer tab whose environment has
//! auto-refresh enabled logs in when the token is missing/stale, and retries
//! once (re-login or refresh-token) if the server answers 401 anyway.

use crate::*;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum FlowEvent {
    /// Normal response events (what `execute_request` emits).
    Net(ResponseEvent),
    /// A login/refresh finished: the UI should store the token in its copy of the environment.
    Token(Result<AuthRefreshOutcome, String>),
}

async fn refresh(env: &mut Option<Environment>, emit: &mut impl FnMut(FlowEvent)) -> Result<(), String> {
    let e = env.as_mut().ok_or_else(|| "no environment selected".to_string())?;
    match perform_auth_refresh(&HTTP_CLIENT, e).await {
        Ok(outcome) => {
            apply_auth_refresh_outcome(e, &outcome);
            emit(FlowEvent::Token(Ok(outcome)));
            Ok(())
        }
        Err(err) => {
            emit(FlowEvent::Token(Err(err.clone())));
            Err(err)
        }
    }
}

pub async fn run_request(
    state: SavedState,
    mut env: Option<Environment>,
    jar: HashMap<String, Vec<CookieEntry>>,
    cancel: Arc<AtomicBool>,
    mut emit: impl FnMut(FlowEvent),
) {
    let is_head = state.method == HttpMethod::HEAD;

    if needs_preventive_refresh(state.auth_type, env.as_ref()) {
        if let Err(err) = refresh(&mut env, &mut emit).await {
            emit(FlowEvent::Net(ResponseEvent::Response(HttpResponse {
                status: "Error".into(),
                body: auth_refresh_failure_body(&err),
                ..Default::default()
            })));
            emit(FlowEvent::Net(ResponseEvent::Done));
            return;
        }
    }

    let mut retried = false;
    loop {
        let (request, url) = build_request(&state, env.as_ref(), &jar);
        let can_retry = !retried && can_retry_after_auth_failure(state.auth_type, false, env.as_ref());

        let (mut held, mut holding, mut first) = (Vec::new(), false, true);
        execute_request(request, url, is_head, cancel.clone(), |ev| {
            if first {
                first = false;
                if can_retry {
                    if let ResponseEvent::Response(r) = &ev {
                        holding = r.status.starts_with("401");
                    }
                }
            }
            if holding {
                held.push(ev);
            } else {
                emit(FlowEvent::Net(ev));
            }
        })
        .await;

        if !holding {
            return;
        }
        retried = true;
        if refresh(&mut env, &mut emit).await.is_err() {
            // refresh failed: show the original 401 rather than hiding it
            for ev in held {
                emit(FlowEvent::Net(ev));
            }
            return;
        }
    }
}
