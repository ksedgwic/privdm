use std::env;
use std::fs;
use std::io::{self, Read};
use std::str::FromStr;

use nostr_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut to = None;
    let mut via = None;
    let mut from = None;
    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--to" => {
                i += 1;
                if i < args.len() {
                    to = Some(args[i].clone());
                }
            }
            "--via" => {
                i += 1;
                if i < args.len() {
                    via = Some(args[i].clone());
                }
            }
            "--from" => {
                i += 1;
                if i < args.len() {
                    from = Some(args[i].clone());
                }
            }
            _ => {}
        }
        i += 1;
    }

    let to = to.expect("missing --to");
    let via = via.expect("missing --via");
    let from = from.expect("missing --from");

    // Read secret key from file
    let secret = fs::read_to_string(from)?.trim().to_owned();
    let secret_key = SecretKey::from_str(&secret)?;
    let keys = Keys::new(secret_key);
    let client = Client::new(keys);

    client.add_relay(via).await?;
    client.connect().await;

    let receiver: PublicKey = PublicKey::from_str(&to)?;

    // Read message from stdin
    let mut msg = String::new();
    io::stdin().read_to_string(&mut msg)?;

    client.send_private_msg(receiver, msg, None).await?;

    Ok(())
}
