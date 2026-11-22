use super::*;

fn agent(pane: &str, status: AgentStatus, changed_ms: Option<u64>) -> Agent {
    Agent {
        pane: pane.into(),
        status,
        changed_ms,
    }
}

fn read_as(agent: &Agent, at: Instant) -> Read {
    Read {
        status: agent.status,
        changed_ms: agent.changed_ms,
        at,
        failed: false,
    }
}

#[test]
fn an_agent_never_read_is_due() {
    let now = Instant::now();
    assert!(due(&agent("p1", AgentStatus::Idle, None), None, now));
}

#[test]
fn an_idle_agent_is_read_again_only_when_it_changes() {
    let start = Instant::now();
    let idle = agent("p1", AgentStatus::Idle, Some(5));
    let read = read_as(&idle, start);
    let later = start + WORKING * 10;
    assert!(!due(&idle, Some(&read), later));
    assert!(due(
        &agent("p1", AgentStatus::Idle, Some(6)),
        Some(&read),
        later
    ));
    assert!(due(
        &agent("p1", AgentStatus::Blocked, Some(5)),
        Some(&read),
        start
    ));
}

#[test]
fn a_working_agent_is_read_every_interval() {
    let start = Instant::now();
    let working = agent("p1", AgentStatus::Working, Some(5));
    let read = read_as(&working, start);
    assert!(!due(&working, Some(&read), start + WORKING / 2));
    assert!(due(&working, Some(&read), start + WORKING));
}

#[test]
fn a_pass_reads_new_agents_first_and_at_most_its_bound() {
    let start = Instant::now();
    let later = start + WORKING * 2;
    let mut agents: Vec<Agent> = (0..MAX_READS + 4)
        .map(|n| agent(&format!("w{n}"), AgentStatus::Working, Some(1)))
        .collect();
    let mut reads: HashMap<String, Read> = agents
        .iter()
        .enumerate()
        .map(|(n, agent)| {
            let at = start + Duration::from_millis(n as u64);
            (agent.pane.clone(), read_as(agent, at))
        })
        .collect();
    agents.push(agent("new", AgentStatus::Idle, None));
    let quiet = agent("quiet", AgentStatus::Idle, Some(1));
    reads.insert(quiet.pane.clone(), read_as(&quiet, start));
    agents.push(quiet);

    let plan: Vec<&str> = plan(&agents, &reads, later)
        .into_iter()
        .map(|agent| agent.pane.as_str())
        .collect();
    assert_eq!(plan.len(), MAX_READS);
    assert_eq!(plan[0], "new");
    assert_eq!(plan[1], "w0");
    assert!(!plan.contains(&"quiet"));
}

#[test]
fn a_failed_read_is_tried_again_after_a_while_even_when_idle() {
    let start = Instant::now();
    let idle = agent("p1", AgentStatus::Idle, Some(5));
    let failed = Read {
        failed: true,
        ..read_as(&idle, start)
    };
    assert!(!due(&idle, Some(&failed), start + RETRY / 2));
    assert!(due(&idle, Some(&failed), start + RETRY));
}
