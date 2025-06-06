# privdm

A small Rust tool to send an ASCII-encoded Nostr private message via a relay.

### Running

Send private direct message to specific relay(s):
```bash
echo "hello world" | \
  privdm \
    --from sample.key \
    --to <RECIPIENT_PUBLIC_KEY|RECIPIENT_NPUB> \
    --via <RELAY_URL_1> \
    --via <RELAY_URL_2> \
    --cc <SENDERS_RELAY_URL>
```

Find (and use) the recipients preferred DM relay(s):
```bash
echo "hello world" | \
  privdm \
    --from sample.key \
    --to <RECIPIENT_NPROFILE> \
    --cc <SENDERS_RELAY_URL>
```

Notes:
- Add `--verbose` to see more info.
- Use `--dry-run` to find the preferred DM relays but not send the message.
- The `--via` option specifies recipient relays and may be specified multiple times.
- The `--cc` option specifies sender relays and may be specified multiple times.

### Prerequisites

- [Rust and Cargo](https://www.rust-lang.org/tools/install) installed (1.65+).
- A Nostr secret key file (ASCII, one key per file). A sample `sample.key` is included in this repo.
- A valid Nostr public key (hex, npub, or nprofile) for the recipient.
- A reachable Nostr relay URL (e.g. `wss://relay.example.com`).

### Building

```bash
cargo build --release
```

### Installing

```bash
cargo install --path . 
```
