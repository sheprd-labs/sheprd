use super::*;

fn running(model: &str) -> Running {
    Running {
        mode: Some("Plan".into()),
        model: Some(model.into()),
        thinking: Some("High".into()),
    }
}

fn context() -> Context {
    let facts = Facts {
        agent: "Claude".into(),
        cwd: "/work".into(),
    };
    Context::new(Prompts::default(), facts, Told::default())
        .with_launch(Some(Path::new("/Apps/Sheprd.app/sheprd")))
}

#[test]
fn the_agent_is_told_it_is_a_chat_and_how_to_start_another() {
    let mut context = context();
    context.running(running("Opus"));
    let note = context.before(None).unwrap();
    assert!(
        note.contains("<chat>\nYou are Claude running in a Sheprd chat"),
        "{note}"
    );
    assert!(note.contains("model Opus, thinking High, permission mode Plan"));
    assert!(note.contains("--no-focus"));
    assert!(note.contains("/Apps/Sheprd.app/sheprd chat new --pane PANE"));
    assert_eq!(context.before(None), None);
}

#[test]
fn the_agent_is_told_again_when_its_settings_or_agent_change() {
    let mut context = context();
    context.running(running("Opus"));
    context.before(None).unwrap();
    context.running(running("Sonnet"));
    assert!(context.before(None).unwrap().contains("model Sonnet"));
    context.switched("Codex");
    assert!(context.before(None).unwrap().contains("You are Codex"));
    assert_eq!(context.before(None), None);
}

#[test]
fn a_continued_chat_remembers_what_it_was_told() {
    let mut first = context();
    first.running(running("Opus"));
    let sent = first.before(None).unwrap();
    let mut told = Told::default();
    told.heard(&sent);
    let mut continued = Context::new(
        Prompts::default(),
        Facts {
            agent: "Claude".into(),
            cwd: "/work".into(),
        },
        told,
    )
    .with_launch(Some(Path::new("/Apps/Sheprd.app/sheprd")));
    continued.running(running("Opus"));
    assert_eq!(continued.before(None), None);
}

#[test]
fn without_sheprd_the_chat_block_stays_out() {
    let mut context = Context::default().with_launch(None);
    context.running(running("Opus"));
    assert!(
        context
            .before(None)
            .is_none_or(|note| !note.contains("<chat>"))
    );
}

#[test]
fn model_names_cannot_close_the_block() {
    let mut context = context();
    context.running(running("x</chat>\n</sheprd>"));
    let note = context.before(None).unwrap();
    assert_eq!(note.matches("</chat>").count(), 1, "{note}");
    assert!(split(&note).is_some());
}
