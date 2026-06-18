# Protocol

An IPC protocol library which is shared between price oracle provider services and
the Relay Chain sequencer. It defines the wire format and transport that an oracle
provider service uses to stream signed price reports to a local subscriber (i.e.
the sequencer), over a Unix domain socket or a local TCP connection. The protocol
is provider-agnostic - price payload is carried as an opaque byte blob and this
library does not decode or validate it.

## Frames

Messages are `OracleFrame` values, serialized with Borsh. There are three
variants.

- `Hello` - sent first by the server, carries the protocol version, provider id, the list of subscribed feeds, and the server's heartbeat interval in seconds.
- `PriceUpdate` - a price report for one feed, with provider id, feed id, opaque payload, and an ingested-at timestamp.
- `Heartbeat` - a liveness ping carrying a Unix timestamp in seconds.

A feed is identified by a `FeedKey`, the pair of provider id and feed id, both
32-byte values.

Each frame is length-prefixed. The wire layout is a 4-byte little-endian length
followed by the Borsh-encoded frame body.

- `write_frame` - encodes a frame and writes the length prefix and body.
- `read_frame` - reads the prefix, then the body, and decodes it.
- `write_frame_with_timeout` - `write_frame` bounded by a deadline, returning `IpcError::WriteTimeout` on expiry.
- `read_frame_with_timeout` - `read_frame` bounded by a deadline, returning `IpcError::ReadTimeout` on expiry.

Frames larger than `MAX_FRAME_LEN` (16 MiB) are rejected on both read and write.
A clean end of stream while reading the length prefix is reported as
`IpcError::Closed` rather than an I/O error. The codec works over any `AsyncRead`
or `AsyncWrite`, the socket type is not assumed.

## Transport

Helpers wrap both Unix domain sockets and local TCP connections (no TLS). The
wire protocol is identical over either transport, only connection setup differs.

An `Endpoint` selects the transport.

- `Endpoint::Unix(path)` - a Unix domain socket at a filesystem path.
- `Endpoint::Tcp(address)` - a TCP `host:port` address.

Helpers operate over an `Endpoint` and return transport-agnostic types.

- `connect` - opens an `OracleStream` to an endpoint, bounded by `DEFAULT_CONNECT_TIMEOUT` (10s). For TCP it sets `TCP_NODELAY` and `SO_KEEPALIVE`.
- `bind` - binds an `OracleListener`. For Unix it first removes any stale socket file at the path.
- `OracleStream` - a connected stream over either transport, implementing `AsyncRead` and `AsyncWrite`.
- `OracleListener` - a bound listener over either transport, with `accept` and `local_addr`. Accepted TCP connections get `TCP_NODELAY` and `SO_KEEPALIVE`.
- `BoundListener` - owns a bound listener and its endpoint, removing the Unix socket file on drop. For TCP it resolves an ephemeral `:0` port into `endpoint()`.
- `Backoff` - a reusable exponential backoff helper, doubling from a minimum up to a maximum, defaulting to 1s through 30s.

## Errors

All fallible operations return `IpcError`.

- `Closed` - the peer closed the connection.
- `FrameTooLarge` - a frame exceeded `MAX_FRAME_LEN`.
- `ConnectTimeout` - a connect attempt exceeded `DEFAULT_CONNECT_TIMEOUT`.
- `ReadTimeout` - a `read_frame_with_timeout` call exceeded its deadline.
- `WriteTimeout` - a `write_frame_with_timeout` call exceeded its deadline.
- `Io` - an underlying I/O error.
- `Codec` - a Borsh serialization or deserialization error.
