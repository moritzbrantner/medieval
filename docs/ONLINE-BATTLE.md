# Online Battle architecture

Online Battle is a first-class game mode beside Campaign. A player should be able to launch Medieval, choose Online Battle, invite one friend, synchronize the required battle content, and start a tactical match without creating or loading a campaign.

## Existing multiplayer foundation

Medieval consumes `moritzbrantner/multiplayer-setup-service` rather than creating a second rendezvous protocol.

For the first two-player version:

- create a private lobby through `POST /lobbies` with `maxParticipants: 2`;
- share the display code or an invite link containing only the public lobby identifier, never another participant's private capability token;
- join through the existing lobby join API;
- let `LobbySession` retain each participant's private capability and authenticate signaling;
- use the browser `LobbySession` WebRTC foundation for the direct peer relationship;
- keep the setup service payload-opaque: it authenticates lobby membership and relays targeted SDP/ICE, but never owns Medieval gameplay state.

The browser source is pinned locally under `web/vendor/multiplayer-setup-service/` from multiplayer-setup-service commit `7028eb41da84de79a9dde88acdee6c4fbc5304d2`. The source files are exact upstream Git blobs and are guarded by deterministic tests so Medieval does not silently fork rendezvous, reconnect, or ICE-recovery behavior.

The setup API defaults to `http://127.0.0.1:8787` for local development. A deployment can set `window.__MEDIEVAL_MULTIPLAYER__.apiBase`; `?setupApi=` is also accepted as an operator/development override. That endpoint is deliberately not copied into invite links: invites contain only `?lobby=<public-display-code>`. ICE and TURN server arrays may likewise be supplied through `window.__MEDIEVAL_MULTIPLAYER__`; credentials must not be put into invite URLs.

The setup-service deployment must also admit the page/webview origin through its `ALLOWED_ORIGINS` policy. Its current defaults cover `https://moritzbrantner.github.io` and localhost browser origins; a packaged Tauri origin must be added explicitly when desktop end-to-end acceptance is enabled. Medieval opens its own Tauri `connect-src` only for secure HTTPS/WSS endpoints plus localhost development, while keeping scripts self-hosted.

## Implemented pre-battle state machine

The first networking slice makes connection and compatibility state explicit:

1. **Idle** — player chooses Host battle or Join invite.
2. **Creating / joining** — Medieval asks the setup service for a private two-player lobby or joins the public lobby identity.
3. **Connecting** — `LobbySession` authenticates signaling and establishes the WebRTC channels.
4. **Peer connected** — the direct reliable/realtime channels are open.
5. **Release check** — Medieval exchanges its own compact release/readiness message on the reliable peer channel.
6. **Ready** — both peers report the same compatible Medieval release and battle protocol and report readiness.
7. **Recovery** — signaling reconnect or ICE recovery stays inside the existing `LobbySession`, preserving the participant identity/capability instead of minting a new one.
8. **Failure** — incompatible releases/protocols or exhausted connection recovery fail closed.

The compact Medieval-owned readiness message is:

```json
{
  "type": "medieval-ready",
  "v": 1,
  "release": "0.1.0",
  "battleProtocol": 1,
  "ready": true
}
```

`release` is intentionally kept in sync with the Medieval workspace release by a test. `battleProtocol` is a separate game-network compatibility version so protocol-breaking changes can fail closed even when general release packaging changes differently.

Only this pre-battle readiness message is sent over the reliable peer channel in this slice. Clicking the enabled Start battle gate emits a local `medieval:battle-start-requested` event for the future tactical layer; it does not send battle commands, world state, inputs, ticks, or simulation traffic.

The setup service does not decide whether two Medieval builds are compatible. `web/online-battle-model.mjs` owns that Medieval decision, while the Online Battle UI only projects the resulting connection/readiness state.

## Asset readiness — next slice

Release/protocol readiness is necessary but not sufficient for a real tactical match. The next networking slice adds trusted content verification:

1. **Manifest verification** — the configured trusted Medieval content manifest is fetched from its authoritative HTTPS/release origin and verified.
2. **Asset scan** — each client determines missing required assets from its verified local cache.
3. **Asset sync** — missing chunks are downloaded from the authoritative HTTPS origin and/or opted-in peers.
4. **Content ready** — every required file has passed exact size, per-chunk SHA-256, and whole-file verification.
5. **Startable** — Start battle is enabled only when both players are ready for the same compatible release, battle protocol, and verified content set.
6. **Battle** — bulk content transfer is stopped or deprioritized so gameplay traffic has priority.

If either player disconnects before Startable, the UI returns to a recoverable lobby/sync state. Reconnect and ICE recovery should continue using the multiplayer service's existing resilience layer rather than creating a new participant identity.

## Asset trust boundary

Peers are byte transports only. They are never trusted to decide which Medieval files, versions, sizes, or hashes are valid.

Medieval opts in to the multiplayer service's content-sharing helpers only in the asset slice, with a trusted release manifest. Required battle content can include maps, terrain data, models, textures, animation data, and audio. Execution-critical logic should normally ship with the installed Medieval release; if logic is ever distributable, it must use the service's signed-manifest verification and fail closed.

Rules:

- manifest authority is configured by Medieval, never supplied by lobby state or a peer;
- received chunks enter reusable storage only after verification against the trusted manifest;
- partially downloaded verified chunks may be resumed and reseeded safely;
- persistent cache entries are release/version scoped;
- corrupt or stale cache content is rejected and evicted;
- players seed only after explicit opt-in;
- P2P sharing is an optimization, not a requirement: HTTPS remains a valid source;
- TURN-relayed paths may disable or strongly cap bulk content transfer;
- gameplay bandwidth always outranks asset distribution.

## Tactical networking horizon

The two-player lobby and asset gate can be implemented before the tactical simulation is feature-complete, but a real Online Battle cannot launch until there is a deterministic tactical battle kernel.

Recommended order:

1. private lobby host/join/invite UI using `LobbySession`;
2. release compatibility and ready-state protocol;
3. verified P2P asset synchronization and progress UI;
4. deterministic tactical battle kernel with fixed simulation ticks;
5. command transport, sequence/tick validation, and periodic state hashes;
6. divergence detection and bounded recovery;
7. reconnect during battle;
8. optional later authoritative server mode if competitive anti-cheat becomes a requirement.

The current slice deliberately makes no anti-cheat claim.
