use std::sync::Mutex;

use bluer::{
    Session,
    agent::{
        Agent, AgentHandle, DisplayPasskey, DisplayPinCode, ReqError, ReqResult,
        RequestAuthorization, RequestConfirmation, RequestPasskey, RequestPinCode,
    },
};
use tokio::sync::oneshot;

use crate::bluetooth::state::{PAIRING_PROMPT, PairingPrompt, PairingRequest};

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

// only one pairing prompt is ever active at a time in this UI, so a single
// slot (rather than a map keyed by device) is sufficient
static PENDING_REPLY: Mutex<Option<oneshot::Sender<PairingReply>>> = Mutex::new(None);

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
    *PENDING_REPLY.lock().unwrap() = Some(tx);
    *PAIRING_PROMPT.write() = Some(prompt);

    let result = match rx.await {
        Ok(reply) => extract(reply).ok_or(ReqError::Rejected),
        Err(_) => Err(ReqError::Rejected),
    };

    *PAIRING_PROMPT.write() = None;
    result
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

/// Shows a PIN code the user needs to type on the *other* device (legacy
/// pairing without a keyboard on their end). There's nothing for us to
/// collect back; we just display it until BlueZ tells us to stop.
async fn handle_display_pin_code(req: DisplayPinCode) -> ReqResult<()> {
    *PAIRING_PROMPT.write() = Some(PairingPrompt {
        address: req.device,
        request: PairingRequest::DisplayPinCode(req.pincode),
    });

    let _ = req.cancel.await;
    *PAIRING_PROMPT.write() = None;
    Ok(())
}

/// Shows a passkey the user needs to type on the *other* device, updating
/// as they type it (`entered` counts digits typed so far there).
async fn handle_display_passkey(req: DisplayPasskey) -> ReqResult<()> {
    *PAIRING_PROMPT.write() = Some(PairingPrompt {
        address: req.device,
        request: PairingRequest::DisplayPasskey {
            passkey: req.passkey,
            entered: req.entered,
        },
    });

    let _ = req.cancel.await;
    *PAIRING_PROMPT.write() = None;
    Ok(())
}

/// Responds to the current pairing prompt, if any.
pub fn respond(reply: PairingReply) {
    if let Some(tx) = PENDING_REPLY.lock().unwrap().take() {
        let _ = tx.send(reply);
    }
}

/// Cancels the current pairing prompt, if any, as a rejection.
pub fn cancel() {
    // dropping the sender causes the awaiting handler's rx.await to error,
    // which await_reply maps to ReqError::Rejected
    PENDING_REPLY.lock().unwrap().take();
    *PAIRING_PROMPT.write() = None;
}
