use log::*;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::str::FromStr;
use std::time::Duration;

use nostr_sdk::event::tag::TagKind;
use nostr_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut dry_run = false;
    let mut verbose = false;
    let mut to = None;
    let mut via: Vec<String> = Vec::new();
    let mut from = None;
    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                if i < args.len() {
                    from = Some(args[i].clone());
                }
            }
            "--to" => {
                i += 1;
                if i < args.len() {
                    to = Some(args[i].clone());
                }
            }
            "--via" => {
                i += 1;
                if i < args.len() {
                    via.push(args[i].clone()); // keep every occurrence
                }
            }
            "--dry-run" => dry_run = true,
            "--verbose" => verbose = true,
            _ => {}
        }
        i += 1;
    }

    if verbose {
        // INFO and up; use RUST_LOG=debug if you ever want more.
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .init();
    }

    // always need from and to
    let from = from.expect("missing --from");
    let to = to.expect("missing --to");

    // load the from secret key
    let secret = fs::read_to_string(from)?.trim().to_owned();
    let secret_key = SecretKey::from_str(&secret)?;
    let keys = Keys::new(secret_key);

    // attempt to decode `--to` as an nprofile
    let (receiver_pk, relay_urls) = if let Ok(profile) = Nip19Profile::from_bech32(&to) {
        info!("using profile {:?} for DM relay discovery", profile);

        // create a dedicated client for discovery
        let client = Client::new(keys.clone());
        for url in &profile.relays {
            client
                .add_relay(url.to_string())
                .await
                .expect("failed to add hint relay");
        }
        client.connect().await;

        let filter = Filter::new()
            .kinds(vec![Kind::Custom(10050)])
            .authors(vec![profile.public_key])
            .limit(1);

        let events = client.fetch_events(filter, Duration::from_secs(5)).await?;
        info!("discovered k=10050: {:?}", events);

        let mut dm_relays = Vec::new();
        if let Some(evt) = events.into_iter().next() {
            for tag in evt.tags {
                if tag.kind() == TagKind::Relay {
                    if let Some(url_str) = tag.content() {
                        if let Ok(url) = RelayUrl::parse(url_str) {
                            info!("found DM relay: {:?}", url);
                            dm_relays.push(url);
                        }
                    }
                }
            }
        }

        let relay_urls = if !dm_relays.is_empty() {
            info!("found profile dm relays {:?}", dm_relays);
            dm_relays
        } else {
            info!(
                "no profile dm relays found, using profile relays {:?}",
                profile.relays
            );
            profile.relays.clone()
        };
        (profile.public_key, relay_urls)
    } else {
        // treat as npub or hex
        let receiver_pk =
            PublicKey::from_str(&to).expect("`--to` was neither valid hex nor npub nor nprofile");
        if via.is_empty() {
            panic!("missing --via for hex/npub input");
        }
        let relay_urls: Vec<RelayUrl> = via
            .iter()
            .map(|v| RelayUrl::parse(v).expect("bad relay URL"))
            .collect();

        (receiver_pk, relay_urls)
    };

    info!("sending DM to {:?} using {:?}", receiver_pk, relay_urls);

    // create a fresh client to send the DM using only the specified relays
    let client = Client::new(keys.clone());
    for url in &relay_urls {
        client
            .add_relay(url.to_string())
            .await
            .expect("couldn’t add relay");
    }
    client.connect().await;

    let mut msg = String::new();
    io::stdin().read_to_string(&mut msg)?;

    if dry_run {
        info!("dry-run selected, exiting w/o sending");
    } else {
        client
            .send_private_msg_to(relay_urls.clone(), receiver_pk, msg, Vec::new())
            .await
            .expect("failed to send DM");
    }

    Ok(())
}
