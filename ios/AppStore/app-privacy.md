# App Privacy answers

The answers for App Store Connect > App Privacy, matching the app's privacy
manifest (`SheprdRemote/PrivacyInfo.xcprivacy`, checked by
`tools/test_privacy.py`) and the policy at https://sheprd.org/privacy/.

"Collected" means sent off the phone to us or a partner and kept longer than
it takes to serve the request. What the app sends to the user's own paired
computers is not collected: we never receive it.

## Data types

| Type | Collected | Linked to the user | Tracking | Purpose |
| --- | --- | --- | --- | --- |
| Identifiers > Device ID | Yes | No | No | App Functionality |
| User Content > Other User Content | Yes | Yes | No | App Functionality |
| Everything else | No | | | |

- **Device ID:** the APNs push token. The phone seals it into a handle only
  the push relay (push.sheprd.org, which we run) can open, and the relay reads
  it while forwarding a push to Apple. The relay never stores or logs it, so
  this is arguably not "collected"; it is declared to be safe, as Apple counts
  identifiers a developer's server receives.
- **Other User Content:** when users enable a cloud voice, spoken text goes to the selected provider under their account and may be retained.
- **Not collected:** contact info, usage data,
  diagnostics (Send Diagnostics goes to the user's own computer), location,
  and any other identifier. There is no analytics or crash-reporting SDK.

## Tracking

No. The app has no third-party SDKs, no advertising identifier, and does not
link data with other companies' data.
