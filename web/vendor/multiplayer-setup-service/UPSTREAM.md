# multiplayer-setup-service browser source pin

These browser files are copied without modification from `moritzbrantner/multiplayer-setup-service` at commit `73cfb7fedde73195a42c7edaada57aaf8372a67b`:

- `lobby-session.js` — Git blob `4cc08e8ee9e10e68d33a4198033d74675236760b`
- `resilient-lobby-session.js` — Git blob `6f9e7e425f88d896915719a8c1d38828d8559e26`
- `game-commands.js` — Git blob `3b9c2b9c1177b128501c979da837d4b16d342c64`

Medieval vendors this exact browser foundation because its Tauri CSP keeps scripts self-hosted. Product-specific release compatibility, readiness semantics, and battle authority remain outside these vendored files.

When updating the pin, replace the source files from one exact upstream revision and update the blob assertions in `web-tests/online-battle-readiness.test.mjs`. Do not patch the vendored transport or command helper locally.
