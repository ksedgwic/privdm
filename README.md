# privdm

A small Rust tool to send an ASCII-encoded Nostr private message via a relay.

### Prerequisites

- [Rust and Cargo](https://www.rust-lang.org/tools/install) installed (1.65+).
- A Nostr secret key file (ASCII, one key per file). A sample `sample.key` is included in this repo.
- A valid Nostr public key (hex) for the recipient.
- A reachable Nostr relay URL (e.g. `wss://relay.example.com`).

### Building

```bash
cargo build --release
```

### Running

```
echo "hello world" | \
  cargo run privdm \
    --from sample.key \
    --to <RECIPIENT_PUBLIC_KEY> \
    --via <RELAY_URL>
```

### Installing

```
cargo install --path . 
```
