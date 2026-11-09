use std::sync::{
    Mutex, PoisonError,
    atomic::{AtomicU64, Ordering},
};

use bluer::{
    Session,
    agent::{
        Agent, AgentHandle, AuthorizeService, DisplayPasskey, DisplayPinCode, ReqError, ReqResult,
        RequestAuthorization, RequestConfirmation, RequestPasskey, RequestPinCode,
    },
};
use tokio::sync::oneshot;

use crate::bluetooth::{
    profile_names::profile_name,
    state::{PAIRING_PROMPT, PairingPrompt, PairingRequest},
};

/// The user's response to whatever [`PairingRequest`] is currently pending.
///
/// Each agent callback below expects a specific variant back; a mismatched
/// one (e.g. the UI sending `Confirm` for a request that expected `Text`) is
/// treated the same as a rejection.
#[derive(Debug)]
pub enum PairingReply {
    Text(String),
    Number(u32),
    Confirm,
}

// only one pairing prompt is ever shown at a time in this UI, so a single
// slot (rather than a map keyed by device) is sufficient; the slot is tagged
// with the generation of the prompt it answers
static PENDING_REPLY: Mutex<Option<(u64, oneshot::Sender<PairingReply>)>> = Mutex::new(None);

/// Generation of the prompt currently in [`PAIRING_PROMPT`], or 0 for none.
///
/// The mutex also serializes every write to [`PAIRING_PROMPT`], so a stale
/// request can never clear a newer prompt between checking and clearing.
static SHOWN_PROMPT: Mutex<u64> = Mutex::new(0);

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Keeps a pairing prompt on screen for as long as its request is alive.
///
/// BlueZ cancels a request by dropping its future, which skips any cleanup
/// written after an await, so clearing happens on drop instead. Each guard
/// only clears state that still belongs to its own request, so overlapping
/// or repeated requests can't wipe each other's prompts.
struct PromptGuard {
    generation: u64,
}

impl PromptGuard {
    /// Shows `prompt`, replacing any prompt from an earlier request.
    fn show(prompt: PairingPrompt) -> Self {
        let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        let mut shown = SHOWN_PROMPT.lock().unwrap_or_else(PoisonError::into_inner);
        *shown = generation;
        *PAIRING_PROMPT.write() = Some(prompt);
        Self { generation }
    }

    /// Makes `tx` the receiver of the user's answer to this prompt.
    fn expect_reply(&self, tx: oneshot::Sender<PairingReply>) {
        *PENDING_REPLY.lock().unwrap_or_else(PoisonError::into_inner) = Some((self.generation, tx));
    }
}

impl Drop for PromptGuard {
    fn drop(&mut self) {
        let mut pending = PENDING_REPLY.lock().unwrap_or_else(PoisonError::into_inner);
        if pending
            .as_ref()
            .is_some_and(|(generation, _)| *generation == self.generation)
        {
            pending.take();
        }
        drop(pending);

        let mut shown = SHOWN_PROMPT.lock().unwrap_or_else(PoisonError::into_inner);
        if *shown == self.generation {
            *shown = 0;
            *PAIRING_PROMPT.write() = None;
        }
    }
}

/// Registers cadenza-shell as a BlueZ pairing agent with full keyboard and
/// display capability (we can both show a code and let the user type one).
///
/// `request_default` mirrors `bluetooth.default_agent`: `false` lets us
/// coexist with another agent (e.g. blueman) that continues handling
/// pairings it initiates, while we still handle pairings we initiate
/// ourselves (BlueZ routes to whichever agent made the request); `true`
/// takes over as the system-wide default.
pub async fn register_agent(
    session: &Session,
    request_default: bool,
) -> bluer::Result<AgentHandle> {
    let agent = Agent {
        request_default,
        request_pin_code: Some(Box::new(|req| Box::pin(handle_request_pin_code(req)))),
        display_pin_code: Some(Box::new(|req| Box::pin(handle_display_pin_code(req)))),
        request_passkey: Some(Box::new(|req| Box::pin(handle_request_passkey(req)))),
        display_passkey: Some(Box::new(|req| Box::pin(handle_display_passkey(req)))),
        request_confirmation: Some(Box::new(|req| Box::pin(handle_request_confirmation(req)))),
        request_authorization: Some(Box::new(|req| Box::pin(handle_request_authorization(req)))),
        // without a handler bluer rejects every request, so untrusted
        // devices could never use their profiles
        authorize_service: Some(Box::new(|req| Box::pin(handle_authorize_service(req)))),
        ..Default::default()
    };

    session.register_agent(agent).await
}

/// Publishes a pairing prompt awaiting a typed reply, then awaits it.
///
/// Returns [`ReqError::Rejected`] if the UI declines, or cancels, or replies
/// with a reply of the wrong shape for this request.
async fn await_reply<T>(
    prompt: PairingPrompt,
    extract: impl FnOnce(PairingReply) -> Option<T>,
) -> ReqResult<T> {
    let (tx, rx) = oneshot::channel();
    let guard = PromptGuard::show(prompt);
    guard.expect_reply(tx);

    match rx.await {
        Ok(reply) => extract(reply).ok_or(ReqError::Rejected),
        Err(_) => Err(ReqError::Rejected),
    }
}

async fn handle_request_pin_code(req: RequestPinCode) -> ReqResult<String> {
    await_reply(
        PairingPrompt {
            address: req.device,
            request: PairingRequest::PinCode,
        },
        |reply| match reply {
            PairingReply::Text(pin) => Some(pin),
            _ => None,
        },
    )
    .await
}

async fn handle_request_passkey(req: RequestPasskey) -> ReqResult<u32> {
    await_reply(
        PairingPrompt {
            address: req.device,
            request: PairingRequest::Passkey,
        },
        |reply| match reply {
            PairingReply::Number(passkey) => Some(passkey),
            _ => None,
        },
    )
    .await
}

async fn handle_request_confirmation(req: RequestConfirmation) -> ReqResult<()> {
    await_reply(
        PairingPrompt {
            address: req.device,
            request: PairingRequest::ConfirmPasskey(req.passkey),
        },
        |reply| match reply {
            PairingReply::Confirm => Some(()),
            _ => None,
        },
    )
    .await
}

async fn handle_request_authorization(req: RequestAuthorization) -> ReqResult<()> {
    await_reply(
        PairingPrompt {
            address: req.device,
            request: PairingRequest::Authorize,
        },
        |reply| match reply {
            PairingReply::Confirm => Some(()),
            _ => None,
        },
    )
    .await
}

/// Asks whether a device that isn't trusted may use a profile.
async fn handle_authorize_service(req: AuthorizeService) -> ReqResult<()> {
    await_reply(
        PairingPrompt {
            address: req.device,
            request: PairingRequest::AuthorizeService(profile_name(req.service)),
        },
        |reply| match reply {
            PairingReply::Confirm => Some(()),
            _ => None,
        },
    )
    .await
}

/// Shows a PIN code the user needs to type on the *other* device (legacy
/// pairing without a keyboard on their end). There's nothing for us to
/// collect back; we just display it until BlueZ tells us to stop.
async fn handle_display_pin_code(req: DisplayPinCode) -> ReqResult<()> {
    let guard = PromptGuard::show(PairingPrompt {
        address: req.device,
        request: PairingRequest::DisplayPinCode(req.pincode),
    });
    hide_when_cancelled(guard, req.cancel);
    Ok(())
}

/// Shows a passkey the user needs to type on the *other* device, updating
/// as they type it (`entered` counts digits typed so far there).
async fn handle_display_passkey(req: DisplayPasskey) -> ReqResult<()> {
    let guard = PromptGuard::show(PairingPrompt {
        address: req.device,
        request: PairingRequest::DisplayPasskey {
            passkey: req.passkey,
            entered: req.entered,
        },
    });
    hide_when_cancelled(guard, req.cancel);
    Ok(())
}

/// Keeps a display-only prompt up until BlueZ cancels it.
///
/// BlueZ waits for the reply before it continues pairing, so display
/// requests are answered right away and the prompt is cleared from a
/// separate task once `cancel` resolves.
fn hide_when_cancelled(guard: PromptGuard, cancel: oneshot::Receiver<()>) {
    relm4::spawn(async move {
        let _ = cancel.await;
        drop(guard);
    });
}

/// Responds to the current pairing prompt, if any.
pub fn respond(reply: PairingReply) {
    // the awaiting request's guard clears the prompt once it finishes
    if let Some((_, tx)) = PENDING_REPLY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        let _ = tx.send(reply);
    }
}

/// Cancels the current pairing prompt, if any, as a rejection.
pub fn cancel() {
    // dropping the sender causes the awaiting handler's rx.await to error,
    // which await_reply maps to ReqError::Rejected
    PENDING_REPLY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();

    // display-only prompts have no reply to drop, so hide them directly
    let mut shown = SHOWN_PROMPT.lock().unwrap_or_else(PoisonError::into_inner);
    *shown = 0;
    *PAIRING_PROMPT.write() = None;
}
