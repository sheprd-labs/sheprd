use super::*;

#[test]
fn credits_merge_without_repeating_a_name() {
    use Credit::*;
    let claude = || Agents("Claude".into());
    assert_eq!(Outside.merge(Outside), Outside);
    assert_eq!(Outside.merge(claude()), claude());
    assert_eq!(claude().merge(Outside), claude());
    assert_eq!(claude().merge(claude()), claude());
    let both = claude().merge(Agents("Codex".into()));
    assert_eq!(both, Agents("Claude, Codex".into()));
    assert_eq!(both.clone().merge(claude()), both);
    assert_eq!(Outside.by(), None);
    assert_eq!(Agents("  ".into()).by(), None);
}
