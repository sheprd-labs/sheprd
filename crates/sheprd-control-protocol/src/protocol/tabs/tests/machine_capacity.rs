use crate::protocol::tabs::{Lending, MachineCapacity, Reply};

#[test]
fn capacity_counts_available_slots_without_underflow() {
    for (in_use, available) in [(0, 2), (1, 1), (2, 0), (3, 0)] {
        let capacity = MachineCapacity::new(in_use);
        assert_eq!(capacity.available, available);
        let reply = Reply::Machines {
            machines: Vec::new(),
            capacity: Some(capacity),
            lending: Some(Lending::OtherVms { count: 1 }),
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert_eq!(serde_json::from_str::<Reply>(&json).unwrap(), reply);
    }
}

#[test]
fn older_machine_lists_have_unknown_capacity() {
    let reply: Reply = serde_json::from_str(r#"{"kind":"machines","machines":[]}"#).unwrap();
    assert!(matches!(reply, Reply::Machines { capacity: None, .. }));
}

#[test]
fn other_programs_vms_fill_slots() {
    let capacity = MachineCapacity::with_others(1, 1);
    assert_eq!(
        (capacity.in_use, capacity.available, capacity.others),
        (2, 0, 1)
    );
    let old: MachineCapacity =
        serde_json::from_str(r#"{"in_use":1,"limit":2,"available":1}"#).unwrap();
    assert_eq!(old.others, 0);
    assert!(Lending::Lends.reason().is_none());
    assert!(Lending::Tart.reason().is_some());
}
