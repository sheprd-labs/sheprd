# The divider where another agent took over a chat's conversation.
# $agent is the agent that took over, such as "Codex"; $kept and $omitted
# count transcript entries.

# It never saw the conversation and was handed it.
chat-handoff-fresh = Continued in { $agent }: { $kept ->
    [one] 1 entry handed over
   *[other] { $kept } entries handed over
}
# It went back to its own earlier session and was handed what it missed.
chat-handoff-resumed = Back to { $agent }: { $kept ->
    [one] 1 new entry handed over
   *[other] { $kept } new entries handed over
}
# Added after either line when entries did not fit.
chat-handoff-omitted = { $line }, { $omitted } left out for length
