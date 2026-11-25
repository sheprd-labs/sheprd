# Why a machine could not start or change, shown in its tab and settings.

error-machine-agent-accessibility = Sheprd Agent cannot post input yet. In the machine, allow it in System Settings > Privacy & Security > Accessibility.
error-machine-agent-io = Lost the connection to Sheprd Agent: { $error }
error-machine-agent-not-running = { $name } is not running. Open it first.
error-machine-agent-picture = Sheprd Agent sent a screenshot this app cannot read.
error-machine-agent-reply = Sheprd Agent sent a reply this app cannot read. It may be older or newer than this app: install it again from the Sheprd Agent disk on its desktop.
error-machine-agent-refused = Sheprd Agent refused: { $message }
error-machine-agent-screen-recording = Sheprd Agent cannot see the screen yet. In the machine, allow it in System Settings > Privacy & Security > Screen & System Audio Recording.
error-machine-agent-too-large = A message of { $bytes } bytes is past the limit.
error-machine-agent-unreachable = Sheprd Agent does not answer in { $name } ({ $reason }). To install it, double-click Install Sheprd Agent.pkg on the Sheprd Agent disk on its desktop.
error-machine-busy = { $name } is in use. Shut it down first.
error-machine-cpus = A machine takes { $min } to { $max } CPUs on this Mac.
error-machine-delete = Cannot delete { $path }: { $error }
error-machine-exists = { $name } already exists.
error-machine-framework = Virtualization refused this machine: { $message }
error-machine-hardware = This machine was made for hardware this Mac does not have.
error-machine-image-not-found = No image is called { $name }.
error-machine-install-cancelled = Stopped. The download is kept, so trying again picks up where it left off.
error-machine-install-download = Cannot download macOS: { $error }
error-machine-install-failed = macOS did not install: { $message }
error-machine-install-fetch = Cannot ask Apple which macOS this Mac runs: { $message }
error-machine-install-incomplete = The download stopped at { $got } of { $expected } GB. Try again to finish it.
error-machine-install-no-room = Making a machine needs { $needed } GB free on this Mac, for the download and the macOS installed from it, and there are { $free } GB. Free up some space and try again.
error-machine-install-not-supported = This Mac cannot run the macOS Apple offers for virtual Macs.
error-machine-install-status = Apple's server answered { $status }. Try again later.
error-machine-invalid-display = A display is fit or a size such as 1920x1200.
error-machine-invalid-name = A machine name takes letters, digits, '-', and '_', and starts with a letter or digit.
error-machine-memory = A machine takes { $min } to { $max } GB of memory on this Mac.
error-machine-no-state-dir = There is no state directory to keep machines in.
error-machine-not-installed = No machine is installed at { $path }.
error-machine-read = Cannot read { $path }: { $error }
error-machine-suspended = { $name } is suspended. Shut it down or forget its saved state first.
error-machine-unsupported = This computer cannot run virtual Macs.
error-machine-write = Cannot write { $path }: { $error }

error-machine-name-mismatch = Open { $installed } instead of { $requested } to use the installed machine name.

error-machine-capacity = No VM slots available: { $in_use } of { $limit } in use. Wait for a VM to stop or reuse an unreserved running VM.

error-machine-remote-answer = The answer from { $host } was not one Sheprd sends: { $error }
error-machine-remote-base64 = The picture from the other computer is not valid base64: { $error }
error-machine-remote-decode = Cannot read the screenshot: { $error }
error-machine-remote-encode = Cannot encode the frame: { $error }
error-machine-remote-missing = No Sheprd is installed on { $host }.
error-machine-remote-not-png = The picture from the other computer is not a PNG of the size it says.
error-machine-remote-not-running = Sheprd is not running on { $host }, so its virtual Macs cannot start.
error-machine-remote-refused = { $host } answered: { $message }
error-machine-remote-request = The request to { $host } could not be written: { $error }
error-machine-remote-run = Cannot reach { $host }: { $error }
error-machine-remote-too-large = The answer from { $host } is larger than a picture can be.
error-machine-remote-too-old = Sheprd on { $host } is too old to lend virtual Macs. Update it there.
