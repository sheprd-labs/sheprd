#![allow(clippy::unwrap_used)]
use super::*;

fn call(op: TabOp) -> LendCall {
    LendCall {
        borrower: "studio".to_owned(),
        owner: "pane 1".to_owned(),
        request: LendRequest::Tab { op },
    }
}

fn refused(answer: LendAnswer) -> bool {
    matches!(
        answer,
        LendAnswer::Response {
            response: Response::Error {
                code: ErrorCode::InvalidRequest,
                ..
            }
        }
    )
}

#[test]
fn a_borrower_never_names_where_a_picture_is_written() {
    let path = "/Users/me/.ssh/authorized_keys".to_owned();
    for op in [
        TabOp::MachineScreenshot {
            name: "default".to_owned(),
            path: path.clone(),
        },
        TabOp::MachinePeek {
            name: "default".to_owned(),
            path: path.clone(),
        },
    ] {
        assert!(refused(answer(call(op)).unwrap()));
    }
}

#[test]
fn a_borrower_asks_only_for_virtual_mac_work() {
    assert!(refused(answer(call(TabOp::Devices)).unwrap()));
}
