//! Native mod-channel integration fixture. Driven by the parent test runner over stdin.
use horizon_mod_speak as speak;
use horizon_protocol::{
    PROTOCOL_VERSION,
    buf::Writer,
    connection::Connection,
    crypto::offline_uuid,
    ids, mod_net,
    packets::{Empty, configuration as cfg, handshake, login, play},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, BufRead, Write},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
fn lock<T>(v: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    v.lock().unwrap()
}
fn join(port: u16, name: &str) -> Connection {
    let mut c = Connection::connect("127.0.0.1", port, Duration::from_secs(10)).unwrap();
    c.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: PROTOCOL_VERSION,
            host: "127.0.0.1".into(),
            port,
            intent: handshake::LOGIN,
        })
        .encode(),
    )
    .unwrap();
    c.send(
        &login::Serverbound::Hello(login::ServerHello {
            name: name.into(),
            uuid: offline_uuid(name),
        })
        .encode(),
    )
    .unwrap();
    loop {
        let (id, b) = c.recv().unwrap();
        match login::Clientbound::decode(id, &b).unwrap() {
            login::Clientbound::Compression(p) => c.set_compression(p.threshold),
            login::Clientbound::LoginFinished(_) => {
                c.send(&login::Serverbound::LoginAcknowledged(Empty).encode())
                    .unwrap();
                break;
            }
            login::Clientbound::Disconnect(p) => panic!("login refused: {}", p.reason_json),
            login::Clientbound::Hello(_) => panic!("fixture requires isolated offline server"),
            _ => {}
        }
    }
    c.send(&cfg::Serverbound::ClientInformation(cfg::ClientInformation::new(2)).encode())
        .unwrap();
    let mut brand = Writer::new();
    brand.string("horizon");
    c.send(
        &cfg::Serverbound::CustomPayload(cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: brand.data,
        })
        .encode(),
    )
    .unwrap();
    loop {
        let (id, b) = c.recv().unwrap();
        let reply = match cfg::Clientbound::decode(id, &b).unwrap() {
            cfg::Clientbound::SelectKnownPacks(p) => cfg::Serverbound::SelectKnownPacks(p),
            cfg::Clientbound::KeepAlive(p) => cfg::Serverbound::KeepAlive(p),
            cfg::Clientbound::Ping(p) => cfg::Serverbound::Pong(p),
            cfg::Clientbound::CodeOfConduct(_) => cfg::Serverbound::AcceptCodeOfConduct(Empty),
            cfg::Clientbound::CookieRequest(p) => {
                cfg::Serverbound::CookieResponse(cfg::CookieResponse {
                    key: p.key,
                    payload: None,
                })
            }
            cfg::Clientbound::FinishConfiguration(_) => {
                c.send(&cfg::Serverbound::FinishConfiguration(Empty).encode())
                    .unwrap();
                return c;
            }
            cfg::Clientbound::Disconnect(_) => panic!("configuration refused"),
            _ => continue,
        };
        c.send(&reply.encode()).unwrap();
    }
}
#[derive(Default)]
struct Results {
    spawned: BTreeSet<String>,
    commands: BTreeSet<String>,
    received: BTreeMap<u32, BTreeSet<String>>,
    errors: Vec<String>,
}
fn pcm() -> Vec<u8> {
    (0..960_i16)
        .flat_map(|i| ((i % 64 - 32) * 300).to_le_bytes())
        .collect()
}
fn main() {
    let port: u16 = std::env::args().nth(1).unwrap().parse().unwrap();
    let results = Arc::new(Mutex::new(Results::default()));
    let mut writers = Vec::new();
    for name in ["Speaker", "Nearby", "Distant"] {
        let c = join(port, name);
        let (mut reader, writer) = c.split();
        let w = Arc::new(Mutex::new(writer));
        writers.push(w.clone());
        let results = results.clone();
        let name = name.to_owned();
        thread::spawn(move || {
            let mut position = [0.0; 3];
            loop {
                let (id, b) = match reader.recv() {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let reply = match id {
                    ids::play::clientbound::KEEP_ALIVE => {
                        match play::Clientbound::decode(id, &b).unwrap() {
                            play::Clientbound::KeepAlive(p) => {
                                Some(play::Serverbound::KeepAlive(p).encode())
                            }
                            _ => None,
                        }
                    }
                    ids::play::clientbound::PING => {
                        match play::Clientbound::decode(id, &b).unwrap() {
                            play::Clientbound::Ping(p) => Some(play::Serverbound::Pong(p).encode()),
                            _ => None,
                        }
                    }
                    ids::play::clientbound::CHUNK_BATCH_FINISHED => Some(
                        play::Serverbound::ChunkBatchReceived(play::ChunkBatchReceived {
                            chunks_per_tick: 4.0,
                        })
                        .encode(),
                    ),
                    ids::play::clientbound::PLAYER_POSITION => {
                        match play::Clientbound::decode(id, &b).unwrap() {
                            play::Clientbound::PlayerPosition(p) => {
                                position = std::array::from_fn(|i| {
                                    if p.relatives & (1 << i) != 0 {
                                        position[i] + p.position[i]
                                    } else {
                                        p.position[i]
                                    }
                                });
                                let mut w = lock(&w);
                                w.send(
                                    &play::Serverbound::AcceptTeleportation(
                                        play::AcceptTeleportation {
                                            id: p.teleport,
                                            x: position[0],
                                            y: position[1],
                                            z: position[2],
                                            yaw: p.yaw,
                                            pitch: p.pitch,
                                        },
                                    )
                                    .encode(),
                                )
                                .unwrap();
                                w.send(&play::Serverbound::PlayerLoaded(Empty).encode())
                                    .unwrap();
                                lock(&results).spawned.insert(name.clone());
                                None
                            }
                            _ => None,
                        }
                    }
                    ids::play::clientbound::COMMANDS => {
                        if let play::Clientbound::Commands(p) =
                            play::Clientbound::decode(id, &b).unwrap()
                        {
                            if p.nodes.iter().any(|n| n.name == "speak") {
                                lock(&results).commands.insert(name.clone());
                            }
                        }
                        None
                    }
                    ids::play::clientbound::SYSTEM_CHAT => {
                        if let play::Clientbound::SystemChat(p) =
                            play::Clientbound::decode(id, &b).unwrap()
                        {
                            let text =
                                horizon_protocol::text::plain(&p.content, &Default::default());
                            println!("CHAT {name}: {text}");
                            io::stdout().flush().unwrap();
                        }
                        None
                    }
                    ids::play::clientbound::CUSTOM_PAYLOAD => {
                        if let play::Clientbound::CustomPayload(p) =
                            play::Clientbound::decode(id, &b).unwrap()
                        {
                            if p.channel == mod_net::CHANNEL {
                                if let Ok(m) = mod_net::decode(&p.data) {
                                    if m.channel == speak::CHANNEL {
                                        match speak::decode_sound_packet(&m.data) {
                                            Some(p) => {
                                                let mut r = lock(&results);
                                                if p.sender != offline_uuid("Speaker")
                                                    || p.data != pcm()
                                                {
                                                    r.errors.push(format!(
                                                        "{name}: corrupt PCM or forged sender"
                                                    ));
                                                }
                                                r.received
                                                    .entry(p.sequence)
                                                    .or_default()
                                                    .insert(name.clone());
                                            }
                                            None => lock(&results)
                                                .errors
                                                .push(format!("{name}: malformed sound frame")),
                                        }
                                    }
                                }
                            }
                        }
                        None
                    }
                    ids::play::clientbound::DISCONNECT => {
                        lock(&results).errors.push(format!("{name}: disconnected"));
                        return;
                    }
                    _ => None,
                };
                if let Some(p) = reply {
                    if lock(&w).send(&p).is_err() {
                        return;
                    }
                }
            }
        });
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        {
            let r = lock(&results);
            if r.spawned.len() == 3 && r.commands.len() == 3 {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "clients did not spawn or receive Speak command tree"
        );
        thread::sleep(Duration::from_millis(50));
    }
    println!("READY: three clients spawned and Speak commands registered");
    io::stdout().flush().unwrap();
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let mut words = line.split_whitespace();
        match words.next().unwrap_or("") {
            "SEND" => {
                let sequence = words.next().unwrap().parse().unwrap();
                let frame = speak::MicPacket {
                    data: pcm(),
                    sequence,
                    whisper: false,
                    group: None,
                };
                let data = speak::encode_mic_packet(&frame).unwrap();
                let payload = mod_net::encode(&mod_net::Message {
                    channel: speak::CHANNEL.into(),
                    data,
                })
                .unwrap();
                lock(&writers[0])
                    .send(
                        &play::Serverbound::CustomPayload(cfg::CustomPayload {
                            channel: mod_net::CHANNEL.into(),
                            data: payload,
                        })
                        .encode(),
                    )
                    .unwrap();
                println!("SENT {sequence}");
            }
            "COMMAND" => {
                let command = words.collect::<Vec<_>>().join(" ");
                lock(&writers[0])
                    .send(&play::Serverbound::ChatCommand(play::ChatCommand { command }).encode())
                    .unwrap();
                println!("COMMAND SENT");
            }
            "CHECK" => {
                let sequence: u32 = words.next().unwrap().parse().unwrap();
                let expected: BTreeSet<String> = words.map(str::to_owned).collect();
                thread::sleep(Duration::from_secs(2));
                let r = lock(&results);
                assert!(r.errors.is_empty(), "{:?}", r.errors);
                let got = r.received.get(&sequence).cloned().unwrap_or_default();
                assert_eq!(got, expected, "wrong PCM delivery for sequence {sequence}");
                println!("PASS {sequence}: PCM delivery {got:?}");
            }
            "STOP" => break,
            _ => panic!("unknown fixture command"),
        }
        io::stdout().flush().unwrap();
    }
    for w in writers {
        lock(&w).shutdown();
    }
}
