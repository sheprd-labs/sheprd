use super::*;
use sheprd_remote_control::control::{
    DeviceId,
    phone::{Outcome, Phone, PhoneState, Refusal},
};

fn phone() -> Phone {
    Phone {
        device: DeviceId("d1".into()),
        name: "iPhone".into(),
        active_ms: 1,
    }
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[test]
fn what_the_daemon_could_not_do_becomes_the_shared_error_codes() {
    assert_eq!(code(&response(Answer::NotFound)), Some(ErrorCode::NotFound));
    assert_eq!(
        code(&response(Answer::Invalid)),
        Some(ErrorCode::InvalidRequest)
    );
    assert_eq!(
        code(&response(Answer::Ambiguous)),
        Some(ErrorCode::InvalidRequest)
    );
    let silent = response(Answer::NoAnswer { phone: phone() });
    assert_eq!(code(&silent), Some(ErrorCode::Timeout));
    let Response::Error { message, .. } = silent else {
        unreachable!();
    };
    assert!(message.starts_with("iPhone did not answer"));
}

#[test]
fn what_the_phone_said_is_answered_as_it_is() {
    for answer in [
        Answer::NoPhone,
        Answer::Phones {
            phones: vec![phone()],
        },
        Answer::Driven {
            phone: phone(),
            outcome: Outcome::Refused {
                reason: Refusal::Locked,
            },
        },
        Answer::Driven {
            phone: phone(),
            outcome: Outcome::Done {
                state: PhoneState::default(),
            },
        },
    ] {
        assert_eq!(response(answer.clone()), Response::Phone { answer },);
    }
}
