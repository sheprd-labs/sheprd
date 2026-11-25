use super::*;

#[test]
fn other_programs_vms_fill_slots_and_stop_lending() {
    let survey = Survey {
        vms: 2,
        tart: false,
    };
    let capacity = survey.capacity(1);
    assert_eq!(
        (capacity.in_use, capacity.available, capacity.others),
        (2, 0, 1)
    );
    assert_eq!(survey.lending(true, 1), Lending::OtherVms { count: 1 });
}

#[test]
fn a_starting_sheprd_vm_is_not_another_programs() {
    // Counted by Sheprd, not yet a process.
    let survey = Survey {
        vms: 0,
        tart: false,
    };
    assert_eq!(survey.capacity(1).others, 0);
    assert_eq!(survey.lending(true, 1), Lending::Lends);
}

#[test]
fn tart_and_the_setting_stop_lending() {
    let tart = Survey { vms: 0, tart: true };
    assert_eq!(tart.lending(true, 0), Lending::Tart);
    let quiet = Survey::default();
    assert_eq!(quiet.lending(false, 0), Lending::Off);
}

#[test]
fn a_tart_folder_with_a_vm_counts_and_hidden_files_do_not() {
    let Ok(folder) = tempfile::tempdir() else {
        return;
    };
    assert!(!keeps_vms(&folder.path().join("missing")));
    assert!(std::fs::write(folder.path().join(".DS_Store"), b"").is_ok());
    assert!(!keeps_vms(folder.path()));
    assert!(std::fs::create_dir(folder.path().join("runner")).is_ok());
    assert!(keeps_vms(folder.path()));
}
