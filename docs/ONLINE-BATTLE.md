# Online Battle architecture

Online Battle is a first-class game mode beside Campaign. A player should be able to launch Medieval, choose Online Battle, invite one friend, synchronize the required battle content, and start a tactical match without creating or loading a campaign.

This document defines the ownership boundary before networking is implemented.

## Existing multiplayer foundation

Medieval should consume `moritzbrantner/multiplayer-setup-service` rather than create a second rendezvous protocol.

For the first two-player version:

- create a private lobby through `POST /lobbies` with `maxParticipants: 2`;
- share the display code or an invite link containing only the public lobby identifier, never another participant's private capability token;
- join through `POST /lobbies/:lobbyId/join`;
- connect each participant with its own capability token;
- use the browser `LobbySession` WebRTC foundation for the direct peer relationship;
- keep the setup service payload-opaque: it authenticates lobby membership and relays targeted SDP/ICE, but never owns Medieval gameplay state.

The first Online Battle implementation is a friendly-match model, not an anti-cheat claim. Medieval's deterministic battle simulation stays Rust-owned. Normal network traffic should be compact battle commands/inputs, fixed tick identifiers, sequence numbers, and periodic state hashes. Full state transfer is reserved for bootstrap or recovery.

## Pre-match state machine

The UI must make readiness explicit rather than letting either player enter a half-synchronized battle.

1. **Idle** — player chooses Host battle or Join invite.
2. **Lobby** — both participants have valid private capabilities and the direct peer connection is being established.
3. **Release check** — both clients identify the Medieval release/battle protocol version they are running.
4. **Manifest verification** — the configured trusted Medieval content manifest is fetched from its authoritative HTTPS/release origin and verified.
5. **Asset scan** — each client determines missing required assets from its verified local cache.
6. **Asset sync** — missing chunks are downloaded from the authoritative HTTPS origin and/or opted-in peers.
7. **Ready** — every required file has passed exact size, per-chunk SHA-256, and whole-file verification. Both players exchange a compact readiness fingerprint.
8. **Startable** — Start battle is enabled only when both players are ready for the same compatible release and battle configuration.
9. **Battle** — bulk content transfer is stopped or deprioritized so gameplay traffic has priority.

If either player disconnects before Startable, the UI returns to a recoverable lobby/sync state. Reconnect and ICE recovery should use the multiplayer service's existing resilience layer rather than creating a new participant identity.

## Asset trust boundary

Peers are byte transports only. They are never trusted to decide which Medieval files, versions, sizes, or hashes are valid.

Medieval opts in to the multiplayer service's content-sharing helpers with a trusted release manifest. Required battle content can include maps, terrain data, models, textures, animation data, and audio. Execution-critical logic should normally ship with the installed Medieval release; if logic is ever distributable, it must use the service's signed-manifest verification and fail closed.

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

## Player-facing synchronization

The synchronization screen should be concrete and calm. Examples of useful states:

- `Waiting for friend…`
- `Connecting directly…`
- `Checking Medieval 0.3.0 battle assets…`
- `Downloading 184 MB · 62% verified`
- `Sharing assets with your friend · 11 MB uploaded`
- `Your files are ready. Waiting for your friend…`
- `Both players ready`

Progress must be based on locally verified chunks, not on peer claims or bytes merely received over the network.

## Tactical networking horizon

The two-player lobby and asset gate can be implemented before the tactical simulation is feature-complete, but a real Online Battle cannot launch until there is a deterministic tactical battle kernel.

Recommended order:

1. lobby host/join/invite UI using `LobbySession`;
2. release compatibility and ready-state protocol;
3. verified P2P asset synchronization and progress UI;
4. deterministic tactical battle kernel with fixed simulation ticks;
5. command transport, sequence/tick validation, and periodic state hashes;
6. divergence detection and bounded recovery;
7. reconnect during battle;
8. optional later authoritative server mode if competitive anti-cheat becomes a requirement.
