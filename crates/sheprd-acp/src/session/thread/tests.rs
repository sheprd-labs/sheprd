use super::*;

#[test]
fn trimmed_ids_are_counted_after_what_an_agent_saw() {
    let trimmed = [
        Trimmed {
            from: EntryId(1),
            to: EntryId(1),
        },
        Trimmed {
            from: EntryId(3),
            to: EntryId(9),
        },
    ];
    assert_eq!(count(&trimmed, None), 8);
    assert_eq!(count(&trimmed, Some(EntryId(5))), 4);
    assert_eq!(count(&trimmed, Some(EntryId(9))), 0);
}
