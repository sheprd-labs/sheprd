use super::super::steps;

#[test]
fn a_value_inside_the_range_steps_by_one_up_to_its_ends() {
    assert_eq!(steps(4, &(2..=8)), (Some(3), Some(5)));
    assert_eq!(steps(2, &(2..=8)), (None, Some(3)));
    assert_eq!(steps(8, &(2..=8)), (Some(7), None));
}

/// An 8 GiB default on a Mac that allows 4 can still be lowered, to 4.
#[test]
fn a_value_above_the_range_steps_down_to_its_end() {
    assert_eq!(steps(8, &(4..=4)), (Some(4), None));
    assert_eq!(steps(12, &(4..=6)), (Some(6), None));
}

#[test]
fn a_value_below_the_range_steps_up_to_its_start() {
    assert_eq!(steps(1, &(4..=6)), (None, Some(4)));
}
