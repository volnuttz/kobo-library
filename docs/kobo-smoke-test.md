# Physical Kobo Smoke-Test Checklist

Use this checklist on every Kobo model/browser combination claimed as
supported. Desktop emulation is useful for debugging but does not count as a
completed device run.

## Support Record

Record one row per device and firmware version. A device is supported only when
all critical checks pass; otherwise record it as best-effort with the failure.

| Date | Tester | Model | Firmware | Result | Notes or issue |
| --- | --- | --- | --- | --- | --- |
| 2026-07-15 | User | Kobo (model not recorded) | Not recorded | Core two-device flow passed | Kobo created; phone joined/uploaded; Kobo saw/downloaded; either device deleted. |

The product owner chooses the initial target models. The release owner must
ensure at least one target Kobo passes before Phase 3 exits and rerun the list
before launch.

## Setup

- Use the physical device's built-in browser with JavaScript enabled.
- Start from a cleared browser cache and record whether cookies/storage are
  available; the core flow must not require either.
- Use HTTPS on the candidate public hostname and verify there is no certificate
  warning or redirect loop.
- Keep a phone available to scan the QR code and exercise cross-device changes.

## Critical Flow

- Open the landing page and create/join a shelf without a blank page or script
  error.
- Confirm text, form controls, QR code, and book actions fit the viewport and
  remain usable with touch.
- Scan the QR code on the phone and confirm it joins exactly the same shelf.
- Upload a small EPUB from the phone; confirm progress and completion states are
  understandable.
- Without manually reloading the Kobo page, confirm the new book appears within
  the documented polling interval.
- Download the book on Kobo and confirm it opens in the reader as a kepub.
- Delete the book from either device and confirm it disappears on the other.
- Put the Kobo to sleep, wake it, and confirm the page recovers and refreshes.
- Exercise empty, invalid-file, conversion-failure, and expired-shelf states;
  confirm each remains navigable and explains the next action.

## Compatibility and Safety Observations

- Confirm the critical flow works without `fetch`, promises, WebSockets,
  service workers, modules, or other modern-only browser APIs.
- Confirm forms and links still provide a usable fallback if JavaScript fails.
- Verify no shelf capability is shown in visible errors, third-party requests,
  screenshots attached to public issues, or server logs collected during the
  run.
- Record page-load and polling behavior on a slow connection and check that the
  device does not become noticeably sluggish over a 30-minute session.

## Evidence

Attach the completed record and issue links to the release notes. Do not put a
real shelf URL or full capability token in evidence; use a redacted URL.
