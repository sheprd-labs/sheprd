//! Control-socket handlers for read-only shared sessions, running on the
//! daemon's Tokio runtime from a control client thread.

use crate::{
    control::{ErrorCode, Request, Response},
    sharing::{GrantId, ShareId, service::ServiceError},
    web::Shared,
};
use std::time::Duration;
use tokio::runtime::Handle;

pub(crate) fn handles(request: &Request) -> bool {
    matches!(
        request,
        Request::ShareList
            | Request::SharePreview { .. }
            | Request::ShareCreate { .. }
            | Request::ShareRevoke { .. }
            | Request::ShareStopAll
    )
}

pub(crate) fn answer(state: &Shared, runtime: &Handle, request: Request) -> Response {
    let Some(sharing) = state.sharing.as_ref() else {
        return Response::Error {
            code: ErrorCode::Unsupported,
            message: "session sharing is unavailable on this daemon".into(),
        };
    };
    match request {
        Request::ShareList => Response::Shares {
            shares: sharing.list(),
        },
        Request::SharePreview {
            pane,
            history,
            include_tools,
        } => match sharing.preview(&state.hub, &pane, history, include_tools) {
            Ok(preview) => Response::SharePreview(preview),
            Err(error) => map_error(error),
        },
        Request::ShareCreate {
            pane,
            history,
            include_tools,
            lifetime_secs,
            relay,
        } => {
            let request = crate::sharing::service::CreateRequest {
                pane: &pane,
                history,
                include_tools,
                lifetime: Duration::from_secs(lifetime_secs),
                relay: relay.as_deref(),
            };
            match runtime.block_on(sharing.create(&state.hub, state.relay.as_deref(), request)) {
                Ok(created) => Response::ShareCreated(created),
                Err(error) => map_error(error),
            }
        }
        Request::ShareRevoke { share, grant } => {
            match runtime.block_on(sharing.revoke(ShareId(share), grant.map(GrantId))) {
                Ok(()) => Response::Done,
                Err(error) => map_error(error),
            }
        }
        Request::ShareStopAll => match runtime.block_on(sharing.stop_all()) {
            Ok(()) => Response::Done,
            Err(error) => map_error(ServiceError::Sharing(error)),
        },
        _ => Response::Error {
            code: ErrorCode::Unsupported,
            message: "unexpected sharing request".into(),
        },
    }
}

fn map_error(error: ServiceError) -> Response {
    let code = match &error {
        ServiceError::UnknownChat(_) => ErrorCode::UnknownChat,
        ServiceError::NoRelays => ErrorCode::NoRelays,
        ServiceError::UnknownShare => ErrorCode::UnknownShare,
        ServiceError::NotReady | ServiceError::Sharing(_) | ServiceError::Relay(_) => {
            ErrorCode::Other
        }
    };
    Response::Error {
        code,
        message: error.to_string(),
    }
}
