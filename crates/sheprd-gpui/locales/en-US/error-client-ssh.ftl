# Failures reaching a host over SSH. The "bridge" is the SSH connection
# that carries the app's traffic to Herdr on the host.

error-client-ssh-cancelled = SSH connection cancelled
error-client-ssh-closed = SSH bridge closed; check host trust, authentication, and remote Herdr installation
error-client-ssh-invalid-socket = invalid name for a socket on the host
error-client-ssh-invalid-target = invalid SSH target (options, controls, and passwords are forbidden)
error-client-ssh-known-hosts-unavailable = SSH has no user known hosts file to save the host key to
error-client-ssh-output-limit = SSH startup output exceeds limit
# $error is why, one of the error-client-ssh-failure messages.
error-client-ssh-refused = SSH bridge closed: { $error }
error-client-ssh-timeout = SSH discovery timed out
error-client-ssh-unchecked = This host's SSH key is not checked
error-client-ssh-unsupported = SSH port forwarding is not supported on this platform

# Why SSH closed the bridge, shown after "SSH bridge closed:" or on its own.
error-client-ssh-failure-auth = authentication failed; load a key into the agent or open a ControlMaster connection in a terminal
error-client-ssh-failure-herdr-missing = no Herdr is installed on the host
error-client-ssh-failure-host-key = the host key is not trusted; verify it by running ssh to the host in a terminal
error-client-ssh-failure-local-network-denied = macOS denied Sheprd access to your local network. If it just asked, allow it and try again; otherwise turn on Sheprd in System Settings › Privacy & Security › Local Network
error-client-ssh-failure-no-route = there is no route to the host
error-client-ssh-failure-other = check host trust, authentication, and remote Herdr installation
error-client-ssh-failure-unreachable = the host is unreachable
error-client-ssh-failure-relay-banned = the relay banned this computer's network address
# $reason is one of the error-client-ssh-ban-reason messages.
error-client-ssh-failure-relay-banned-for = the relay banned this computer's network address for { $reason }
# $ban is one of the relay-banned messages above; $minutes is a whole number.
error-client-ssh-failure-relay-banned-lifts = { $ban }; it lifts in about { $minutes } min

# What earned a relay ban, shown after "banned … for".
error-client-ssh-ban-reason-bad-proof = key proofs that did not check out
error-client-ssh-ban-reason-bandwidth = moving more data than the relay allows in an hour
error-client-ssh-ban-reason-connection-cap = too many connections at once
error-client-ssh-ban-reason-limited = going over the relay's rate limits
error-client-ssh-ban-reason-malformed = data that is not the relay's protocol
error-client-ssh-ban-reason-manual = a decision of the relay's operator
error-client-ssh-ban-reason-probe = connections held open without a request
