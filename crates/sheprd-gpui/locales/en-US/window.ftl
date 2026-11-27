# The main window's own cards over the terminal area: the reconnecting card
# and the daemon's product announcement. Host names and the announcement's
# title and body come from the user and the daemon.

# The card over a dropped connection; $host is the device's name.
window-reconnecting = Reconnecting to { $host }
window-reconnect-now = Reconnect now
# What the prompt over the window says while a host waits on a Tailscale check;
# $host is the device's name.
window-tailscale-verify = Verify your identity with { $host }
# What the check asks for, under that title. The address follows on its own
# line, so nothing here names it and no sentence is ever cut around it.
window-tailscale-check = Tailscale SSH needs you to prove your identity. Open the address below, and this host connects.
# Opens the sign-in address a pending check printed. The address is copied too,
# because handing it to a browser can fail without saying so.
window-open-browser = Open in browser
# The same card while the device is being removed on purpose.
window-removing = Removing { $host }
window-device-being-removed = It is being removed.
# The announcement's caption; $version is the Sheprd release it is about.
window-announcement = Sheprd { $version }
window-announcement-preview = Sheprd { $version } · preview
window-dismiss = Dismiss
# $error is why the daemon could not mark the announcement seen.
window-dismiss-announcement-failed = Dismiss announcement: { $error }
