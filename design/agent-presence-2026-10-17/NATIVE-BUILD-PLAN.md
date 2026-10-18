# Accepted native integration — 17 October 2026

The user approved revision2 and explicitly requested full integration, retaining an independent design-intent reviewer. This accepts the enclosing conversation, segmented roster, calibrated hats and scrolling album design. The original quoted concerns remain independent acceptance cases.

## Ownership

- Shared presentation, portraits, face/hat/prop art, compact album and camera-scaled presence: `atlas-shell` owners.
- Slate command routing, camera/gesture adapter, profile and conversation storage, harness lifecycle: `apps/slate::agent_presence`.
- Existing pure witnessed-action/grant reducer and authored command journal remain the execution owners.
- Direct human messaging needs an actual transport: repository inspection found none. `docs/collab.md` already names `atlas-relay` as the future protocol/relay owner. Implement its bounded participant-messaging slice there, independent of board synchronization or scene interpretation.

## Required dependency delta before execution

Human messages require encryption, endpoint authentication, bounded WebSocket framing and actual delivery acknowledgments. Existing dependencies cannot supply a self-hosted encrypted participant relay. Add the already-designed `atlas-relay` capability package (library plus small binary), used by Slate. Keep it UI-free and free of scene/agent authority.

Reuse already-locked Rustls0.23.45 with its existing ring provider, serde, crossbeam-channel, sha2 and url. Add pinned tungstenite0.28.0 for the standard WebSocket codec and rcgen0.13.2 for first-run local self-signed certificates. These are new capabilities, not a build-repair workaround. No toolchain, target, profile, linker or Rust-flag change is authorized by this plan. The dependency graph/lockfile will grow and Windows may require approval of newly built artifacts; retain any denial and do not evade it.

Primary API references: <https://docs.rs/rustls/0.23.45/rustls/>, <https://docs.rs/tungstenite/0.28.0/tungstenite/>, <https://docs.rs/rcgen/0.13.2/rcgen/>.

The relay uses authenticated ephemeral room membership only. Local profiles/history remain separate; joining never grants board access, source access or agent authority. Recipient persistence must precede a delivery acknowledgment. No synthetic human delivery or shared-board claim.

## Validation

Derive acceptance cases from the complete original brain dump and latest quoted concerns. Record implementation, automated checks, actual native interaction and user acceptance separately. Capture the exact delivered executable identity and actual open menus/conversations at720px height, photo/name editing, history/drafts after restart, hats, provider handoffs,25/100/200% zoom, moving-avatar double-click and retirement/restore. Preserve the prior browser evidence. Unavailable controls or blocked checks remain explicitly open.

## Retained build events

The first relay test invocation stopped before executing `target/release/build/getrandom-145ba1234f451472/build-script-build.exe` with Windows OS error5. `target/presence-native-relay-tests.log` retains the denial. No configuration or protection was changed. A subsequent independent normal build in this shared checkout produced `libatlas_relay-46b4f43d13fc812f.rlib` at14:49 local time; that is concrete changed external build state, not evidence that relay tests passed.

Codex then crashed. The user's explicit instruction to continue resumes the same implementation. Sources, approved design, original acceptance scope and denial evidence remain intact. Agents were restarted with ownership preserved. The same focused shared-shell test command was restarted after the crash, with the established MSVC setup and default release profile. It initially waited behind another chat's `shape_palettes` fixture build. No test or native UX pass is inferred from compilation elsewhere.

Independent intent review found missing empty-board train/generator creation, granted image metadata/content, and preservation of human drafts during chat-train relay. Those are implementation requirements, not deferred design requests. DRY review requires extracting pure preparation in existing creation/media owners before exposing targeted constructors; all staging, witnessed motion, journal, stale-input and grant checks remain in their existing owners.
