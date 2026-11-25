# Notes for App Review

Paste the section below into App Store Connect > App Review Information >
Notes. The rest of this file is for us.

---

Sheprd is a remote control for Sheprd, a free desktop app for macOS, Linux,
and Windows (https://sheprd.org) that runs AI coding agents and terminals on
the user's own computer. The iPhone app pairs with that computer and shows what
its agents are doing; nothing runs on the phone itself. There is no account and
no sign-in.

To review without a computer: on the first screen, tap Explore the Demo. It
shows two made-up computers with agents, a question waiting for an answer, a
chat, and a terminal, all generated on the phone. Nothing is sent while the
demo runs; tapping an action explains that. A bar at the bottom ends the demo
or opens pairing.

To review with a computer: install Sheprd from https://sheprd.org, turn on
Remote control in its Settings, choose Pair device, and scan the code from the
app's Pair a Computer button.

About the app's capabilities:

- Remote control of the user's own computers (Guideline 4.2.7): the app only
  connects to computers the user paired by scanning a code shown on that
  computer and approving it there. It is not a thin client for a cloud
  service, and no other person's computer can be reached.
- Background modes: remote-notification clears an alert answered elsewhere;
  bluetooth-central lets the computer know the phone is near, which the user
  turns on per computer (Settings > Notifications > Hold Alerts at the Desk),
  so alerts wait while they are at the desk.
- App Transport Security: the app connects over HTTPS to the user's own
  computers by IP address (local network or Tailscale). Each computer has its
  own certificate authority, and the app pins that authority's fingerprint,
  received when pairing. ATS cannot check such addresses against public roots,
  so arbitrary loads are allowed and every connection is checked against the
  pinned fingerprint instead.
- Time-sensitive notifications: only questions an agent is waiting on.
- GitHub sign-in is optional and only reads the user's own pull requests and
  checks; it is not an account for this app.
- Microphone and speech recognition transcribe dictation on the device.

---

## Before submitting

- Contact email: sheprd@pen.so (`metadata.toml`). Add a name and phone in App Review Information.
- Attach a short screen recording of pairing a real computer, in case the
  reviewer asks for one.
- Check that the build's What to Test notes are not pasted here: they list
  commits, not features.
