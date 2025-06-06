use log::*;
use nostr_sdk::RelayMessage;
use nostr_sdk::RelayPoolNotification::Message as PoolMsg;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::str::FromStr;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration, Instant};
use url::Url;

use nostr_sdk::event::tag::TagKind;
use nostr_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut dry_run = false;
    let mut verbose = false;
    let mut to = None;
    let mut via: Vec<String> = Vec::new();
    let mut cc: Vec<String> = Vec::new();
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
            "--cc" => {
                i += 1;
                if i < args.len() {
                    cc.push(args[i].clone()); // keep every occurrence
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

    let mut cc_relays: Vec<RelayUrl> = cc
        .iter()
        .map(|u| RelayUrl::parse(u).expect("bad --cc relay URL"))
        .collect();

    // attempt to decode `--to` as an nprofile
    let (receiver_pk, mut relay_urls) = if let Ok(profile) = Nip19Profile::from_bech32(&to) {
        info!("using profile {:?} for DM relay discovery", profile);
        let relay_urls = discover_nip17_relays(&profile.public_key, &profile.relays).await?;
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

    if !cc_relays.is_empty() {
        // find sender's declared NIP-17 relays
        info!("discovering sender's DM relays");
        cc_relays = discover_nip17_relays(&keys.public_key, &cc_relays).await?;
    }

    // merge in the sender's relays
    cc_relays.retain(|r| !relay_urls.contains(r));
    relay_urls.extend(cc_relays);

    info!("sending DM to {:?} using {:?}", receiver_pk, relay_urls);

    let mut msg = String::new();
    io::stdin().read_to_string(&mut msg)?;

    if dry_run {
        info!("dry-run selected, exiting w/o sending");
    } else {
        // create a fresh client to send the DM using only the specified relays
        let client = Client::new(keys.clone());
        for url in &relay_urls {
            client
                .add_relay(url.to_string())
                .await
                .expect("couldn’t add relay");
        }
        client.connect().await;

        // make sure wer can connect to all of the relays
        let mut ok_relays = Vec::new();
        for url in relay_urls {
            // get “host” and “port” out of the RelayUrl
            let u = Url::parse(&url.to_string()).unwrap();
            let host = u.host_str().unwrap();
            let port = u.port_or_known_default().unwrap();

            match timeout(Duration::from_secs(1), TcpStream::connect((host, port))).await {
                Ok(Ok(_)) => {
                    ok_relays.push(url);
                }
                _ => {
                    log::warn!("dropping unreachable relay {}", url);
                }
            }
        }
        let relay_urls = ok_relays;

        let mut notifs = client.notifications();
        let mut pending: HashSet<_> = relay_urls.iter().cloned().collect();
        let deadline = Instant::now() + Duration::from_secs(5);

        let event_id = client
            .send_private_msg_to(relay_urls.clone(), receiver_pk, msg, Vec::new())
            .await?;

        while !pending.is_empty() && Instant::now() < deadline {
            match timeout(
                deadline.saturating_duration_since(Instant::now()),
                notifs.recv(),
            )
            .await
            {
                // ───────────── relay answered with OK/ERR ─────────────
                Ok(Ok(PoolMsg {
                    relay_url,
                    message:
                        RelayMessage::Ok {
                            event_id: eid,
                            status,
                            message: txt,
                        },
                })) if eid == *event_id => {
                    if status {
                        info!("{relay_url} accepted DM");
                    } else {
                        warn!("{relay_url} rejected DM: {txt:?}");
                    }
                    pending.remove(&relay_url);
                }

                // some other pool notification; ignore
                Ok(Ok(note)) => {
                    info!("saw pool notification: {:?}", note);
                }

                // channel closed or we fell behind; stop waiting
                Ok(Err(err)) => {
                    warn!("saw inner error: {:?}", err);
                    break;
                }

                // this recv call hit the overall deadline – loop again
                Err(err) => {
                    warn!("saw outer error: {:}", err);
                } // keep waiting until `while` condition fails
            }
        }

        for url in pending {
            warn!("no response from {url}");
        }
    }

    Ok(())
}

async fn discover_nip17_relays(
    pubkey: &PublicKey,
    seed_relays: &Vec<RelayUrl>,
) -> Result<Vec<RelayUrl>> {
    info!(
        "{}: looking for NIP-17 DM relays using seed relays: {:?}",
        pubkey, seed_relays
    );

    // create a dedicated client for discovery
    let client = Client::default();
    for url in seed_relays {
        client
            .add_relay(url.to_string())
            .await
            .expect("failed to add seed relay");
    }
    client.connect().await;

    let filter = Filter::new()
        .kinds(vec![Kind::Custom(10050)])
        .authors(vec![*pubkey])
        .limit(1);

    let events = client.fetch_events(filter, Duration::from_secs(5)).await?;
    debug!("discovered k=10050: {:?}", events);

    let mut dm_relays = Vec::new();
    if let Some(evt) = events.into_iter().next() {
        for tag in evt.tags {
            if tag.kind() == TagKind::Relay {
                if let Some(url_str) = tag.content() {
                    if let Ok(url) = RelayUrl::parse(url_str) {
                        info!("{}: found NIP-17 DM relay: {:?}", pubkey, url);
                        dm_relays.push(url);
                    }
                }
            }
        }
    }

    let relay_urls = if !dm_relays.is_empty() {
        info!("{}: found NIP-17 DM relays {:?}", pubkey, dm_relays);
        dm_relays
    } else {
        info!("{}: no NIP-17 DM relays found, using seed relays", pubkey);
        seed_relays.clone()
    };
    Ok(relay_urls)
}
