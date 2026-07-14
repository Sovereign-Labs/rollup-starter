# Protocol

An IPC protocol library which is shared between price oracle provider services and
the Relay Chain sequencer. It defines the wire format and transport that an oracle
provider service uses to stream signed price reports to a local subscriber (i.e.
the sequencer), over a TCP connection. The protocol is provider-agnostic, the price
payload is carried as an opaque byte blob and this library does not decode or validate it.

## Frames

Messages are `OracleFrame` values, serialized with Borsh.
There are three variants.

- `Hello` - Sent first by the server, carries the protocol version, provider id, and the list of subscribed feeds.
- `PriceUpdate` - A price report for one feed, with feed id, opaque payload, and a source timestamp in Unix milliseconds taken from the upstream report.
- `Heartbeat` - A periodic liveness message.

A feed is identified by a `FeedKey`, the pair of provider id and feed id, both 32-byte values.

The heartbeat cadence is fixed by the protocol. A server must send some frame at
least every `HEARTBEAT_INTERVAL` (5s), and a subscriber should treat a stream
with no frame for `READ_DEADLINE` (15s) as dead.

Each frame is length-prefixed.
The wire layout is a 4-byte little-endian length followed by the Borsh-encoded frame body.

- `write_frame` - Encodes a frame and writes the length prefix and body.
- `read_frame` - Reads the prefix, then the body, and decodes it.
- `write_frame_with_timeout` - `write_frame` bounded by a deadline, returning `IpcError::WriteTimeout` on expiry.
- `read_frame_with_timeout` - `read_frame` bounded by a deadline, returning `IpcError::ReadTimeout` on expiry.

Frames larger than `MAX_FRAME_LEN` (256 KiB) are rejected on both read and write.
A clean end of stream while reading the length prefix is reported as `IpcError::Closed`
rather than an I/O error. The codec works over any `AsyncRead` or `AsyncWrite`
the socket type is not assumed.

## Transport

Helpers wrap a local TCP connection (no TLS).

- `connect` - Opens an `OracleStream` to a `host:port` address, bounded by `DEFAULT_CONNECT_TIMEOUT` (10s). It sets `TCP_NODELAY` and `SO_KEEPALIVE`.
- `bind` - Binds an `OracleListener` to a `host:port` address.
- `OracleStream` - A connected TCP stream, implementing `AsyncRead` and `AsyncWrite`.
- `OracleListener` - A bound listener, with `accept` and `local_addr`. Accepted connections get `TCP_NODELAY` and `SO_KEEPALIVE`.
- `BoundListener` - Owns a bound listener and its resolved address, exposing it through `address()` (resolving an ephemeral `:0` port).
- `Backoff` - A reusable exponential backoff helper, doubling from a minimum up to a maximum, defaulting to 1s through 30s.

## Errors

All fallible operations return `IpcError`.

- `Closed` - The peer closed the connection.
- `FrameTooLarge` - A frame exceeded `MAX_FRAME_LEN`.
- `ConnectTimeout` - A connect attempt exceeded `DEFAULT_CONNECT_TIMEOUT`.
- `ReadTimeout` - A `read_frame_with_timeout` call exceeded its deadline.
- `WriteTimeout` - A `write_frame_with_timeout` call exceeded its deadline.
- `Io` - An underlying I/O error.
- `Codec` - A Borsh serialization or deserialization error.
