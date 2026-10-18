# Participant relay

This is the participant messaging slice of the self-hosted capability named in
`docs/collab.md`. It does not implement board synchronization or confer board,
source, filesystem, provider, or agent authority.

`Host::start("127.0.0.1:0".parse()?)` makes one ephemeral room. An explicit
`Invite::with_address(IP:port)` can advertise a reachable remote address when
the host is deliberately bound to that interface. No DNS, discovery, account,
firewall change, automatic connection, or public service is involved. The small
`atlas-relay` binary accepts `--bind` and `--advertise` and stops on Enter.

Share the encoded invite only with intended participants, through a trusted
channel. It contains a random bearer credential and the public self-signed
certificate plus its SHA-256 pin. The client checks that pin, then uses ordinary
Rustls signature, lifetime and server-name validation against that certificate
as its sole trust root. It never disables certificate verification or changes
the machine trust store. Keys, invitations and relay messages remain in memory;
there is no relay persistence. Restart creates a new room and certificate.

Anyone holding the invite can assert a participant identity; this small
account-free room does not certify a human's real-world identity. A duplicate
live participant ID is refused. The server binds subsequent sends, receipts and
profile updates to that authenticated connection, with no client sender field.

`Client::connect(invite, Profile::new(id, name))` returns a bounded worker handle.
Poll `events()` on the UI thread; `send`, `ack`, `update_profile`, disconnect
and Drop are nonblocking. An app owns durable profiles, drafts and messages.
`Profile.avatar` optionally carries at most2048 encoded image bytes; omitting
it is backwards compatible. The app should create a tiny32px JPEG on a worker
and decode received bytes on a worker with strict image-dimension and memory
limits. The relay checks only byte length and adds no image dependency. A full
20-person roster with maximum portraits still fits the256KiB frame ceiling.
Persist an incoming message before calling `ack(message_id)`. `Accepted` means
queued to a live recipient; `Ack` alone means recipient-confirmed persistence.
A lost connection before the receipt is **unconfirmed**, never delivered.
There is no offline queue, automatic retry, or automatic resend. The app should
deduplicate durable incoming history by message ID before acknowledging again.

Limits: 20 live/handshaking connections, 128 items per channel, 32 KiB UTF-8 text,
256 KiB WebSocket frames/messages, 20 participant commands per second,
32 outstanding unacknowledged sends per participant, 1024
recent delivery records, four-second absolute TLS/HTTP/auth handshake deadline,
three-second TCP connect, and heartbeat expiry after 20 seconds. Stalled UI or
network consumers disconnect rather than silently discard incoming messages.
Room-unique message IDs bind a sender, recipient and content hash. Duplicates
within the bounded window do not create a second incoming message. Successful
and unconfirmed records can age out; durable UI deduplication remains necessary.
An explicit retry after a disconnect requeues an unconfirmed envelope under the
same ID; a recipient that already saved it acknowledges again without a new turn.

No network I/O runs on the frame loop. Host setup should also be requested on a
worker because binding and certificate generation are synchronous. No scene,
agent, UI, or OS-secret-store dependencies exist in this crate.
