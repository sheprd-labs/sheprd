# `sheprd chat pr`: an agent reading or adding to its chat's pull requests.

error-chat-pull-requests-closed = The chat ended the connection before it answered
error-chat-pull-requests-connect = The chat is not running or cannot be reached
# $variable is the environment variable's name.
error-chat-pull-requests-invalid-endpoint = { $variable } does not name a chat
error-chat-pull-requests-invalid-url = Not a GitHub pull request address, such as https://github.com/OWNER/REPO/pull/7
error-chat-pull-requests-io = Talking to the chat failed
error-chat-pull-requests-no-chat = { $variable } is not set: run this as the agent of a Sheprd chat
error-chat-pull-requests-no-endpoint-dir = Neither XDG_STATE_HOME nor HOME is set, so the chat cannot be found
error-chat-pull-requests-protocol = The chat sent a message this version cannot read
error-chat-pull-requests-refused = The chat refused the connection
# $reason is the chat's own reason, already in the reader's language.
error-chat-pull-requests-rejected = The chat refused: { $reason }
error-chat-pull-requests-timeout = The chat did not answer in time
