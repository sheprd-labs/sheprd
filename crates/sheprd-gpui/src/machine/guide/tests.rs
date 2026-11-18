use super::*;

#[test]
fn signing_in_comes_first_and_the_user_says_when() {
    assert_eq!(Step::of(false, None, false), Step::SignIn);
    assert_eq!(Step::of(true, None, false), Step::Install);
}

#[test]
fn the_agents_answer_settles_the_permissions_in_order() {
    assert_eq!(
        Step::of(true, Some((false, false)), false),
        Step::Accessibility
    );
    assert_eq!(
        Step::of(true, Some((false, true)), false),
        Step::Accessibility
    );
    assert_eq!(
        Step::of(true, Some((true, false)), false),
        Step::ScreenRecording
    );
    assert_eq!(Step::of(true, Some((true, true)), false), Step::Save);
}

#[test]
fn an_image_kept_ends_the_guide() {
    assert_eq!(Step::of(true, Some((true, true)), true), Step::Ready);
}

#[test]
fn the_listed_steps_come_in_order_and_ready_is_past_them() {
    assert!(Step::LISTED.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(Step::LISTED.iter().all(|step| *step < Step::Ready));
}
