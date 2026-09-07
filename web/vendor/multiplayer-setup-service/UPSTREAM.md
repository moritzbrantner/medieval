# multiplayer-setup-service browser source pin

These browser files are copied without modification from `moritzbrantner/multiplayer-setup-service` at commit `7028eb41da84de79a9dde88acdee6c4fbc5304d2`:

- `lobby-session.js` — Git blob `4cc08e8ee9e10e68d33a4198033d74675236760b`
- `resilient-lobby-session.js` — Git blob `6f9e7e425f88d896915719a8c1d38828d8559e26`

Medieval vendors this exact browser foundation because its Tauri CSP keeps scripts self-hosted. Product-specific release compatibility and readiness remain outside these vendored files.

When updating the pin, replace the source files from one exact upstream revision and update the blob assertions in `web-tests/online-battle-readiness.test.mjs`. Do not patch the vendored transport locally.
