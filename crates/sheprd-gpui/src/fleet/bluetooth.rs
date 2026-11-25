//! The fleet around a Bluetooth pairing (`docs/relay-plan.md`, Adding a
//! nearby Sheprd from another network): what Add Device > Nearby Sheprd
//! does once the other computer is paired and saved, when that computer's
//! Allow brought a fleet along. The page itself only pairs; joining a
//! fleet is Settings > Fleet's. The `desktop-pair` child checked
//! everything the other computer answered.

use super::{Error, Fleet, flows, remote};
use crate::remote::Supervisor;
use gpui::App;
use sheprd_client::ConnectTarget;
use sheprd_remote_control::control::fleet::{
    Method,
    bluetooth::ApprovedFleet,
    sync::{Answer, Apply, Ask},
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The session saved devices use.
const SESSION: &str = "default";

/// Finishes what the pairing left for the fleet, over the SSH it opened to
/// `target`: admits the other computer's bundle and hands it the log, or
/// takes the log from the other computer that admitted this one. A
/// failure shows in Settings > Fleet; the two stay paired either way.
pub(crate) fn paired(cx: &mut App, target: String, fleet: ApprovedFleet) {
    let Some(client) = Supervisor::client(cx) else {
        Fleet::pairing_done(cx, Some(Error::NoDaemon.to_string()));
        return;
    };
    let ours = Fleet::member_of(cx);
    let target = ConnectTarget::Ssh {
        target,
        session: SESSION.to_owned(),
    };
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        // The other computer may still be starting what its Allow set off,
        // or the relays may blink: tried again a few times, as each step is
        // safe to repeat (an admitted bundle is admitted once, and a log is
        // handed over from the heads each side holds).
        let mut outcome = Err(Error::Unexpected);
        for wait in RETRIES {
            executor.timer(wait).await;
            outcome = finish(&client, &executor, &target, &fleet, ours.as_deref()).await;
            if outcome.is_ok() {
                break;
            }
        }
        cx.update(|cx| {
            Fleet::pairing_done(cx, outcome.err().map(|error| error.to_string()));
            Fleet::refresh(cx);
        });
    })
    .detach();
}

/// How long `paired` waits before each try.
const RETRIES: [Duration; 5] = [
    Duration::ZERO,
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(300),
];

/// One try at what the pairing left for the fleet.
async fn finish(
    client: &crate::remote::supervisor::Client,
    executor: &gpui::BackgroundExecutor,
    target: &ConnectTarget,
    fleet: &ApprovedFleet,
    ours: Option<&str>,
) -> Result<(), Error> {
    match fleet.clone() {
        ApprovedFleet::Joining(joining) => {
            let ours = ours.ok_or(Error::NoDaemon)?;
            flows::daemon(
                client,
                Ask::Admit {
                    bundle: joining.bundle,
                    signature: joining.signature,
                    method: Method::Bluetooth,
                },
            )
            .await?;
            flows::hand_over(client, executor, target, ours).await
        }
        ApprovedFleet::Joined { fleet } => {
            take_over(client, executor, target, &fleet).await?;
            match flows::daemon(client, Ask::Status).await? {
                Answer::Status { view: Some(view) } if view.fleet == fleet => Ok(()),
                _ => Err(Error::NotAdmitted),
            }
        }
    }
}

/// Takes `fleet`'s log from `target`, which admitted this computer, one
/// answer at a time: this computer's daemon keeps the history as far as it
/// arrived and answers its heads, and takes it as its log once it holds
/// the admission, which only a fleet this computer signed itself into can.
async fn take_over(
    client: &crate::remote::supervisor::Client,
    executor: &gpui::BackgroundExecutor,
    target: &ConnectTarget,
    fleet: &str,
) -> Result<(), Error> {
    let mut ours = Vec::new();
    for _ in 0..flows::MAX_PARTS {
        let ask = Apply {
            fleet: Some(fleet.to_owned()),
            heads: ours,
            ..Apply::default()
        };
        let remote = target.clone();
        let part = executor
            .spawn(async move { remote::apply(&remote, &ask, &AtomicBool::new(false)) })
            .await?;
        if part.ops.is_empty() {
            return Ok(());
        }
        let more = part.more;
        ours = flows::applied(
            client,
            Apply {
                fleet: Some(fleet.to_owned()),
                heads: part.heads,
                ops: part.ops,
                ..Apply::default()
            },
        )
        .await?
        .heads;
        if !more {
            return Ok(());
        }
    }
    Err(Error::TooLarge)
}
