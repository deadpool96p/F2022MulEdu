use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use blaze_core::frame::{BlazeHeader, BlazePacket, MessageType};
use blaze_core::framing::{try_parse_packet, Framing};
use blaze_tdf::{TdfList, TdfMap, TdfStruct, TdfType, TdfValue};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

struct Logger {
    file: Arc<Mutex<std::fs::File>>,
}

impl Logger {
    fn new(path: &str) -> Self {
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("Failed to open blaze_auth.log");
        Self {
            file: Arc::new(Mutex::new(f)),
        }
    }

    fn log(&self, msg: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("[{:>10}] {}\n", now, msg);
        print!("{}", line);
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("blaze_auth_server=debug,blaze_core=debug")
        .init();

    let logger = Arc::new(Logger::new("blaze_auth.log"));
    let bind_addr = "127.0.0.1:42128";
    let listener = TcpListener::bind(bind_addr).await?;
    logger.log(&format!("Blaze Auth Server listening on {}", bind_addr));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let logger_cloned = Arc::clone(&logger);
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, peer_addr, logger_cloned.clone()).await {
                logger_cloned.log(&format!("Client {} error: {:?}", peer_addr, e));
            }
        });
    }
}

async fn handle_client(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    logger: Arc<Logger>,
) -> Result<(), Box<dyn std::error::Error>> {
    logger.log(&format!("Accepted connection from {}", peer_addr));

    let mut buf = Vec::with_capacity(4096);
    let mut temp = [0u8; 4096];
    let mut framing = Framing::Fire2;
    let mut detected_framing = false;

    loop {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            logger.log(&format!("Connection closed by client {}", peer_addr));
            break;
        }

        buf.extend_from_slice(&temp[..n]);

        // Detect TLS handshake or framing on first packet
        if !detected_framing && buf.len() >= 3 {
            if buf[0] == 0x16 && buf[1] == 0x03 {
                logger.log(&format!(
                    "WARNING: Detected TLS ClientHello (0x16 0x03) from client {}! (SSL not expected on plain 42128)",
                    peer_addr
                ));
            } else if buf.len() >= 12 {
                if buf[3..5] == [0, 0] {
                    framing = Framing::Fire2;
                    logger.log(&format!("Detected Fire2 framing from {}", peer_addr));
                } else {
                    framing = Framing::Legacy;
                    logger.log(&format!("Detected Legacy FIRE framing from {}", peer_addr));
                }
                detected_framing = true;
            }
        }

        while let Some(packet) = try_parse_packet(&mut buf, framing) {
            let comp = packet.header.component;
            let cmd = packet.header.command;
            let seq = packet.header.seq_no;
            let msg_num = packet.header.message_number;

            logger.log(&format!(
                "<- RECV Comp=0x{:04X} Cmd=0x{:04X} Type={:?} Seq={} MsgNum={} PayloadFields={}",
                comp,
                cmd,
                packet.header.msg_type,
                seq,
                msg_num,
                packet.payload.fields.len()
            ));

            let response_packet = handle_packet(&packet, &logger)?;
            let encoded = match framing {
                Framing::Fire2 => response_packet.encode_16()?,
                Framing::Legacy => response_packet.encode()?,
            };

            logger.log(&format!(
                "-> SEND Comp=0x{:04X} Cmd=0x{:04X} Type={:?} Len={} bytes",
                response_packet.header.component,
                response_packet.header.command,
                response_packet.header.msg_type,
                encoded.len()
            ));

            stream.write_all(&encoded).await?;
            stream.flush().await?;
        }
    }

    Ok(())
}

fn handle_packet(req: &BlazePacket, logger: &Logger) -> Result<BlazePacket, Box<dyn std::error::Error>> {
    let comp = req.header.component;
    let cmd = req.header.command;

    match (comp, cmd) {
        // Util::preAuth (0x0009:0x0007)
        (0x0009, 0x0007) => {
            logger.log("[Blaze Auth] Handled Util::preAuth -> Returning QoS & Component config");
            let mut qos_site = TdfStruct::new();
            qos_site.set("PSA", TdfValue::String("127.0.0.1".to_string()));
            qos_site.set("PSP", TdfValue::Integer(17504));

            let mut qos = TdfStruct::new();
            qos.set("BWPS", TdfValue::String("bio-iad".to_string()));
            qos.set("LATT", TdfValue::Integer(0));
            qos.set(
                "LTPS",
                TdfValue::Map(TdfMap::new(
                    TdfType::String,
                    TdfType::Struct,
                    vec![(TdfValue::String("bio-iad".to_string()), TdfValue::Struct(qos_site))],
                )),
            );
            qos.set("NVID", TdfValue::Integer(0));
            qos.set("NVLO", TdfValue::Integer(5_000_000));
            qos.set("TIME", TdfValue::Integer(5_000_000));

            let cids = vec![
                TdfValue::Integer(1),     // Authentication
                TdfValue::Integer(4),     // GameManager
                TdfValue::Integer(7),     // Stats
                TdfValue::Integer(9),     // Util
                TdfValue::Integer(10),    // CensusData
                TdfValue::Integer(16),    // Entitlements (0x0010)
                TdfValue::Integer(25),    // AssociationLists
                TdfValue::Integer(30722), // UserSessions (0x7802)
            ];

            let mut resp = TdfStruct::new();
            resp.set("CIDS", TdfValue::List(TdfList::new(TdfType::Integer, cids)));
            resp.set("CONF", TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, Vec::new())));
            resp.set("QOSS", TdfValue::Struct(qos));

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            resp.set("STIM", TdfValue::Integer(now));

            Ok(BlazePacket::response(&req.header, resp)?)
        }

        // Authentication::login (0x0001:0x0001) or Authentication::createWalUserSession (0x0001:230)
        (0x0001, 0x0001) | (0x0001, 0x00E6) | (0x0001, 40) => {
            logger.log("[Blaze Auth] Handled Authentication::login -> Granting Persona 'Player' (PID=1, UID=1)");
            let mut persona = TdfStruct::new();
            persona.set("DSNM", TdfValue::String("Player".to_string()));
            persona.set("LAST", TdfValue::Integer(0));
            persona.set("PID", TdfValue::Integer(1));
            persona.set("XREF", TdfValue::Integer(0));
            persona.set("XTYP", TdfValue::Integer(0));

            let mut resp = TdfStruct::new();
            resp.set("BUID", TdfValue::Integer(1));
            resp.set("FRST", TdfValue::Integer(0));
            resp.set("KEY", TdfValue::String("fifa22_blaze_session_token_12345".to_string()));
            resp.set("LLOG", TdfValue::Integer(0));
            resp.set("MAIL", TdfValue::String("player@f22emu.local".to_string()));
            resp.set("PDTL", TdfValue::Struct(persona));
            resp.set("UID", TdfValue::Integer(1));

            Ok(BlazePacket::response(&req.header, resp)?)
        }

        // Entitlements::getEntitlements (0x0010:0x0020 or 0x0001:32)
        (0x0010, 0x0020) | (0x0001, 32) => {
            logger.log("[Blaze Auth] Handled Entitlements::getEntitlements -> Granting ONLINE_ACCESS=1");
            let mut ent = TdfStruct::new();
            ent.set("NAME", TdfValue::String("ONLINE_ACCESS".to_string()));
            ent.set("VAL", TdfValue::Integer(1));
            ent.set("EXP", TdfValue::Integer(0));

            let mut resp = TdfStruct::new();
            resp.set(
                "ENTL",
                TdfValue::List(TdfList::new(TdfType::Struct, vec![TdfValue::Struct(ent)])),
            );
            resp.set(
                "NLST",
                TdfValue::List(TdfList::new(
                    TdfType::String,
                    vec![TdfValue::String("ONLINE_ACCESS".to_string())],
                )),
            );

            Ok(BlazePacket::response(&req.header, resp)?)
        }

        // Util::ping (0x0009:0x0002)
        (0x0009, 0x0002) => {
            logger.log("[Blaze Auth] Handled Util::ping");
            if req.header.msg_type == MessageType::Ping {
                Ok(BlazePacket::ping_reply(&req.header)?)
            } else {
                Ok(BlazePacket::response(&req.header, TdfStruct::new())?)
            }
        }

        // Util::fetchClientConfig (0x0009:0x0001)
        (0x0009, 0x0001) => {
            logger.log("[Blaze Auth] Handled Util::fetchClientConfig");
            let mut resp = TdfStruct::new();
            resp.set("CONF", TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, Vec::new())));
            Ok(BlazePacket::response(&req.header, resp)?)
        }

        // Util::postAuth (0x0009:0x0008)
        (0x0009, 0x0008) => {
            logger.log("[Blaze Auth] Handled Util::postAuth");
            Ok(BlazePacket::response(&req.header, TdfStruct::new())?)
        }

        // UserSessions::updateNetworkInfo (0x7802:0x0008)
        (0x7802, 0x0008) | (0x7802, 0x0002) => {
            logger.log("[Blaze Auth] Handled UserSessions update");
            Ok(BlazePacket::response(&req.header, TdfStruct::new())?)
        }

        // Generic catch-all: respond with success (RecordResponse) so the client never stalls
        _ => {
            logger.log(&format!(
                "[Blaze Auth] Catch-all success for Comp=0x{:04X} Cmd=0x{:04X}",
                comp, cmd
            ));
            Ok(BlazePacket::response(&req.header, TdfStruct::new())?)
        }
    }
}
