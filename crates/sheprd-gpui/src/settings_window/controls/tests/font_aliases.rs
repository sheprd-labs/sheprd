use super::*;

#[test]
fn localized_aliases_find_the_displayed_family_without_duplicate_results() {
    let names = vec!["Consolas".to_owned(), "等距更纱黑体 SC".to_owned()];
    let aliases = HashMap::from([(
        "等距更纱黑体 sc".to_owned(),
        vec!["sarasa mono sc".to_owned()],
    )]);

    assert_eq!(filter_fonts(&names, &aliases, " SARASA mono "), vec![1]);
    assert_eq!(filter_fonts(&names, &aliases, "更纱"), vec![1]);
    assert_eq!(filter_fonts(&names, &aliases, "sc"), vec![1]);
    assert_eq!(filter_fonts(&names, &aliases, ""), vec![0, 1]);
    assert!(filter_fonts(&names, &aliases, "missing").is_empty());

    let english_names = vec!["Sarasa Mono SC".to_owned()];
    let aliases = HashMap::from([(
        "sarasa mono sc".to_owned(),
        vec!["等距更纱黑体 sc".to_owned()],
    )]);
    assert_eq!(filter_fonts(&english_names, &aliases, "更纱"), vec![0]);
}
