mod tls;

use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use blaze_core::frame::{BlazePacket, MessageType};
use blaze_core::framing::{try_parse_packet, Framing};
use blaze_tdf::{TdfList, TdfMap, TdfStruct, TdfType, TdfValue};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

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
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter("blaze_auth_server=debug,blaze_core=debug")
        .init();

    let logger = Arc::new(Logger::new("blaze_auth.log"));

    // Ensure f22_ca_modulus.bin is present in working directory
    if !std::path::Path::new("f22_ca_modulus.bin").exists() {
        if let Err(e) = std::fs::write("f22_ca_modulus.bin", tls::CA_MODULUS) {
            logger.log(&format!("Failed to write f22_ca_modulus.bin: {}", e));
        } else {
            logger.log("Exported embedded f22_ca_modulus.bin to working directory");
        }
    }

    let tls_acceptor = Arc::new(tls::create_tls_acceptor()?);
    logger.log("TLS Acceptor initialized successfully with EA Frostbite / DirtySDK cert chain");

    let listener_42128 = TcpListener::bind("127.0.0.1:42128").await?;
    logger.log("Blaze Auth Server listening on 127.0.0.1:42128");

    let listener_42127 = match TcpListener::bind("127.0.0.1:42127").await {
        Ok(l) => {
            logger.log("Blaze Redirector listening on 127.0.0.1:42127");
            Some(l)
        }
        Err(e) => {
            logger.log(&format!("Could not bind 127.0.0.1:42127: {}", e));
            None
        }
    };

    let listener_443 = match TcpListener::bind("127.0.0.1:443").await {
        Ok(l) => {
            logger.log("HTTPS / QoS Sink listening on 127.0.0.1:443");
            Some(l)
        }
        Err(e) => {
            logger.log(&format!("Could not bind 0.0.0.0:443: {}", e));
            None
        }
    };

    // Spawn CRL HTTP Server on port 8099 (serves f22_ca.crl for Windows Crypt32 / WinHTTP revocation engine)
    match TcpListener::bind("127.0.0.1:8099").await {
        Ok(crl_listener) => {
            logger.log("CRL Server listening on http://127.0.0.1:8099/f22_ca.crl");
            let l = Arc::clone(&logger);
            tokio::spawn(async move {
                let crl_bytes = tls::CA_CRL;
                loop {
                    match crl_listener.accept().await {
                        Ok((mut stream, peer)) => {
                            let mut buf = [0u8; 1024];
                            let _ = stream.read(&mut buf).await;
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/pkix-crl\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                                crl_bytes.len()
                            );
                            let mut full_resp = resp.into_bytes();
                            full_resp.extend_from_slice(crl_bytes);
                            let _ = stream.write_all(&full_resp).await;
                            let _ = stream.flush().await;
                            l.log(&format!("[CRL] Served f22_ca.crl ({} bytes) to {}", crl_bytes.len(), peer));
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        Err(e) => {
            logger.log(&format!("Could not bind CRL port 8099: {}", e));
        }
    }

    // Spawn QoS UDP Responders on 17498 and 17504
    for qos_port in [17498, 17504] {
        let l = Arc::clone(&logger);
        tokio::spawn(async move {
            match tokio::net::UdpSocket::bind(format!("0.0.0.0:{}", qos_port)).await {
                Ok(sock) => {
                    l.log(&format!("QoS UDP Responder listening on 0.0.0.0:{}", qos_port));
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match sock.recv_from(&mut buf).await {
                            Ok((len, src)) => {
                                if len < 4 { continue; }
                                let req_id = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                                let (ref_ip, ref_port) = match src {
                                    SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
                                    SocketAddr::V6(_) => ([127, 0, 0, 1], src.port()),
                                };
                                let now_us = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u32;
                                let mut resp = Vec::with_capacity(len.max(20));
                                resp.extend_from_slice(&buf[0..4]); // Request ID
                                resp.extend_from_slice(&ref_ip);    // Reflected IP
                                resp.extend_from_slice(&ref_port.to_be_bytes()); // Reflected Port
                                resp.extend_from_slice(&now_us.to_be_bytes());   // Timestamp
                                resp.extend_from_slice(&[0, 0]); // NAT type = 0 (OPEN)
                                if len > 16 {
                                    resp.extend_from_slice(&buf[16..len]);
                                }
                                let _ = sock.send_to(&resp, src).await;
                                l.log(&format!("QoS: Responded to probe from {} (ReqId 0x{:08X})", src, req_id));
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(e) => {
                    l.log(&format!("Could not bind QoS UDP port {}: {}", qos_port, e));
                }
            }
        });
    }

    let logger_42128 = Arc::clone(&logger);
    let tls_acceptor_42128 = Arc::clone(&tls_acceptor);
    tokio::spawn(async move {
        loop {
            match listener_42128.accept().await {
                Ok((stream, peer_addr)) => {
                    let l = Arc::clone(&logger_42128);
                    let tls = Arc::clone(&tls_acceptor_42128);
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, peer_addr, tls, l.clone()).await {
                            l.log(&format!("Client {} error on 42128: {:?}", peer_addr, e));
                        }
                    });
                }
                Err(e) => {
                    logger_42128.log(&format!("Accept error on 42128: {:?}", e));
                }
            }
        }
    });

    if let Some(l_42127) = listener_42127 {
        let logger_42127 = Arc::clone(&logger);
        let tls_acceptor_42127 = Arc::clone(&tls_acceptor);
        tokio::spawn(async move {
            loop {
                match l_42127.accept().await {
                    Ok((stream, peer_addr)) => {
                        let l = Arc::clone(&logger_42127);
                        let tls = Arc::clone(&tls_acceptor_42127);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, peer_addr, tls, l.clone()).await {
                                l.log(&format!("Client {} error on 42127: {:?}", peer_addr, e));
                            }
                        });
                    }
                    Err(e) => {
                        logger_42127.log(&format!("Accept error on 42127: {:?}", e));
                    }
                }
            }
        });
    }

    if let Some(l_443) = listener_443 {
        let logger_443 = Arc::clone(&logger);
        let tls_acceptor_443 = Arc::clone(&tls_acceptor);
        tokio::spawn(async move {
            loop {
                match l_443.accept().await {
                    Ok((stream, peer_addr)) => {
                        let l = Arc::clone(&logger_443);
                        let tls = Arc::clone(&tls_acceptor_443);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, peer_addr, tls, l.clone()).await {
                                l.log(&format!("Client {} error on 443: {:?}", peer_addr, e));
                            }
                        });
                    }
                    Err(e) => {
                        logger_443.log(&format!("Accept error on 443: {:?}", e));
                    }
                }
            }
        });
    }

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}

fn extract_sni_from_client_hello(buf: &[u8]) -> Option<String> {
    if buf.len() < 44 { return None; }
    let session_id_len = buf[43] as usize;
    let mut pos = 44 + session_id_len;
    if pos + 2 > buf.len() { return None; }
    let cipher_suites_len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
    pos += 2 + cipher_suites_len;
    if pos + 1 > buf.len() { return None; }
    let comp_methods_len = buf[pos] as usize;
    pos += 1 + comp_methods_len;
    if pos + 2 > buf.len() { return None; }
    let ext_len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
    pos += 2;
    let end = (pos + ext_len).min(buf.len());
    while pos + 4 <= end {
        let ext_type = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let ext_size = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
        pos += 4;
        if ext_type == 0 { // server_name
            if pos + 5 <= end {
                let name_len = u16::from_be_bytes([buf[pos + 3], buf[pos + 4]]) as usize;
                if pos + 5 + name_len <= end {
                    return String::from_utf8(buf[pos + 5..pos + 5 + name_len].to_vec()).ok();
                }
            }
        }
        pos += ext_size;
    }
    None
}

async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: Arc<TlsAcceptor>,
    logger: Arc<Logger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut peek_buf = [0u8; 512];
    let n = stream.peek(&mut peek_buf).await?;

    if n >= 2 && peek_buf[0] == 0x16 && peek_buf[1] == 0x03 {
        let sni = extract_sni_from_client_hello(&peek_buf[..n]);
        logger.log(&format!(
            "Detected TLS ClientHello from {} (SNI: {:?}). Negotiating TLS handshake...",
            peer_addr, sni
        ));
        let tls_stream = tls_acceptor.accept(stream).await?;
        logger.log(&format!("TLS handshake established with {}", peer_addr));
        handle_blaze_stream(tls_stream, peer_addr, logger).await?;
    } else {
        logger.log(&format!("Plain TCP connection from {}", peer_addr));
        handle_blaze_stream(stream, peer_addr, logger).await?;
    }

    Ok(())
}

async fn handle_blaze_stream<S>(
    mut stream: S,
    peer_addr: SocketAddr,
    logger: Arc<Logger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut buf = Vec::with_capacity(4096);
    let mut temp = [0u8; 4096];

    loop {
        let n = stream.read(&mut temp).await?;
        if n == 0 {
            logger.log(&format!("Connection closed by client {}", peer_addr));
            break;
        }

        buf.extend_from_slice(&temp[..n]);

        let hex_preview = temp[..n.min(16)]
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        logger.log(&format!("RECV {} bytes from {}: [{}]", n, peer_addr, hex_preview));

        // Check for HTTP requests (telemetry / River data / HTTP redirector over TLS)
        if buf.starts_with(b"POST ") || buf.starts_with(b"GET ") || buf.starts_with(b"PUT ") || buf.starts_with(b"HEAD ") {
            let req_str = String::from_utf8_lossy(&buf);
            let first_line = req_str.lines().next().unwrap_or("").to_string();
            let response = generate_http_response(&req_str);
            stream.write_all(&response).await?;
            stream.flush().await?;
            logger.log(&format!("-> Handled HTTP request [{}] from {} (replied {} bytes)", first_line, peer_addr, response.len()));
            buf.clear();
            break;
        }

        // Parse Blaze packets (try Fire2 first, fallback to Legacy)
        loop {
            let (packet, used_framing) = if let Some(p) = try_parse_packet(&mut buf, Framing::Fire2) {
                (p, Framing::Fire2)
            } else if let Some(p) = try_parse_packet(&mut buf, Framing::Legacy) {
                (p, Framing::Legacy)
            } else {
                break;
            };

            let comp = packet.header.component;
            let cmd = packet.header.command;
            let seq = packet.header.seq_no;
            let msg_num = packet.header.message_number;

            logger.log(&format!(
                "<- RECV Comp=0x{:04X} Cmd=0x{:04X} Type={:?} Seq={} MsgNum={} Framing={:?} PayloadFields={}\n   Payload: {:?}",
                comp,
                cmd,
                packet.header.msg_type,
                seq,
                msg_num,
                used_framing,
                packet.payload.fields.len(),
                packet.payload
            ));

            let response_packet = handle_packet(&packet, &logger)?;
            let encoded = match used_framing {
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

fn handle_packet(req: &BlazePacket, logger: &Logger) -> Result<BlazePacket, Box<dyn std::error::Error + Send + Sync>> {
    let comp = req.header.component;
    let cmd = req.header.command;

    match (comp, cmd) {
        // Redirector::getServerInstance (0x0005:0x0001)
        (0x0005, 0x0001) => {
            logger.log("[Blaze Auth] Handled Redirector::getServerInstance -> Directing to 127.0.0.1:42128 and advertising qosapi");
            let mut ip_addr = TdfStruct::new();
            ip_addr.set("HOST", TdfValue::String("spring18.gosredirector.ea.com".to_string()));
            ip_addr.set("IP", TdfValue::Integer(2130706433));
            ip_addr.set("PORT", TdfValue::Integer(42128));

            let mut resp = TdfStruct::new();
            resp.set(
                "ADDR",
                TdfValue::Union(blaze_tdf::TdfUnion::with_member(
                    0,
                    blaze_tdf::TdfUnion::DEFAULT_MEMBER_TAG,
                    TdfValue::Struct(ip_addr),
                )),
            );
            resp.set("SECU", TdfValue::Integer(0)); // Plain TCP
            resp.set("XREF", TdfValue::Integer(0));
            resp.set("MSIZ", TdfValue::Integer(131072));
            resp.set("AMNT", TdfValue::String("fifa-2022-pc".to_string()));

            // Service instances (Blaze + qosapi)
            let mut blaze_inst = TdfStruct::new();
            blaze_inst.set("TYPE", TdfValue::Integer(0));
            blaze_inst.set("SNAM", TdfValue::String("blaze".to_string()));
            blaze_inst.set("SNMS", TdfValue::List(TdfList::new(TdfType::String, vec![TdfValue::String("blaze".to_string())])));
            blaze_inst.set("SVID", TdfValue::Integer(1));
            blaze_inst.set("HOST", TdfValue::String("spring18.gosredirector.ea.com".to_string()));
            blaze_inst.set("IP", TdfValue::Integer(2130706433));
            blaze_inst.set("PORT", TdfValue::Integer(42128));
            blaze_inst.set("SRIP", TdfValue::Integer(2130706433));
            blaze_inst.set("NMAS", TdfValue::Integer(u32::MAX as u64));
            blaze_inst.set("SRPT", TdfValue::Integer(42128));
            blaze_inst.set("DSPT", TdfValue::Integer(42128));

            let mut qos_inst = TdfStruct::new();
            qos_inst.set("TYPE", TdfValue::Integer(0));
            qos_inst.set("SNAM", TdfValue::String("qosapi".to_string()));
            qos_inst.set("SNMS", TdfValue::List(TdfList::new(TdfType::String, vec![TdfValue::String("qosapi".to_string())])));
            qos_inst.set("SVID", TdfValue::Integer(2));
            qos_inst.set("HOST", TdfValue::String("spring18.gosredirector.ea.com".to_string()));
            qos_inst.set("IP", TdfValue::Integer(2130706433));
            qos_inst.set("PORT", TdfValue::Integer(443));
            qos_inst.set("SRIP", TdfValue::Integer(2130706433));
            qos_inst.set("NMAS", TdfValue::Integer(u32::MAX as u64));
            qos_inst.set("SRPT", TdfValue::Integer(443));
            qos_inst.set("DSPT", TdfValue::Integer(443));

            resp.set("INST", TdfValue::Struct(blaze_inst.clone()));
            resp.set("INSTS", TdfValue::List(TdfList::new(TdfType::Struct, vec![
                TdfValue::Struct(blaze_inst),
            ])));

            Ok(BlazePacket::response(&req.header, resp)?)
        }

        // Util::preAuth (0x0009:0x0007)
        (0x0009, 0x0007) => {
            logger.log("[Blaze Auth] Handled Util::preAuth -> Returning QoS & Component config");

            let mut resp = TdfStruct::new();
            resp.set("ASRC", TdfValue::String("5091917".to_string()));
            resp.set("CLID", TdfValue::String("FIFA22-PC-EMULATOR-BLAZE".to_string()));
            resp.set("ESRC", TdfValue::String("5091917".to_string()));
            resp.set("INST", TdfValue::String("fifa-2022-pc".to_string()));
            resp.set("MAID", TdfValue::Integer(0));
            resp.set("MINR", TdfValue::Integer(0));
            resp.set("NASP", TdfValue::String("cem_ea_id".to_string()));
            resp.set("PILD", TdfValue::String(String::new()));
            resp.set("PLAT", TdfValue::String("pc".to_string()));
            resp.set("RSRC", TdfValue::String("5091917".to_string()));
            resp.set("SVER", TdfValue::String("Blaze 15.1.1.10.3 (CL# 5091917)\n".to_string()));

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
            resp.set("CIDS", TdfValue::List(TdfList::new(TdfType::Integer, cids)));

            // CONF: FetchConfigResponse struct wrapping CONF map
            let entry = |k: &str, v: &str| (TdfValue::String(k.to_string()), TdfValue::String(v.to_string()));
            let conf_entries = vec![
                entry("associationListSkipInitialSet", "1"),
                entry("autoReconnectEnabled", "0"),
                entry("bytevaultHostname", "bytevault.gameservices.ea.com"),
                entry("bytevaultPort", "443"),
                entry("bytevaultSecure", "true"),
                entry("cachedUserRefreshInterval", "1s"),
                entry("clientUserMetricsUpdateRate", "60000"),
                entry("connIdleTimeout", "40s"),
                entry("defaultRequestTimeout", "20s"),
                entry("maxReconnectAttempts", "30"),
                entry("nonResumableTimeoutScale", "2.0"),
                entry("nucleusConnect", "https://accounts.ea.com"),
                entry("nucleusConnectTrusted", "https://accounts2s.ea.com"),
                entry("nucleusPortal", "https://signin.ea.com"),
                entry("nucleusProxy", "https://gateway.ea.com"),
                entry("Override_ProtoHttp_LoginStateMachine_DedicatedServer_vers", "770,0,NULL"),
                entry("pingPeriod", "20s"),
                entry("userManagerMaxCachedUsers", "0"),
                entry("voipHeadsetUpdateRate", "1000"),
                entry("xblTokenUrn", "accounts.ea.com"),
                entry("xboxOneStringValidationUri", "client-strings.xboxlive.com"),
            ];
            let mut conf_wrapper = TdfStruct::new();
            conf_wrapper.set("CONF", TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, conf_entries)));
            resp.set("CONF", TdfValue::Struct(conf_wrapper));

            // QOSS
            let mut qos_site = TdfStruct::new();
            qos_site.set("PSA", TdfValue::String("spring18.gosredirector.ea.com".to_string()));
            qos_site.set("PSP", TdfValue::Integer(17504));

            let mut qos_struct = TdfStruct::new();
            qos_struct.set("BWPS", TdfValue::Struct(qos_site.clone()));
            qos_struct.set("LNP", TdfValue::Integer(0));
            qos_struct.set(
                "LTPS",
                TdfValue::Map(TdfMap::new(
                    TdfType::String,
                    TdfType::Struct,
                    vec![(TdfValue::String("bio-iad".to_string()), TdfValue::Struct(qos_site))],
                )),
            );
            qos_struct.set("TIME", TdfValue::Integer(0));
            resp.set("QOSS", TdfValue::Struct(qos_struct));

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
            let mut tele = TdfStruct::new();
            tele.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
            tele.set("ANON", TdfValue::Integer(1));
            tele.set("DISA", TdfValue::String(String::new()));
            tele.set("FILT", TdfValue::String(String::new()));
            tele.set("LOC", TdfValue::Integer(0));
            tele.set("NOOK", TdfValue::String(String::new()));
            tele.set("PORT", TdfValue::Integer(443));
            tele.set("SDLY", TdfValue::Integer(0));
            tele.set("SKEY", TdfValue::String(String::new()));
            tele.set("SPCT", TdfValue::Integer(100));

            let mut tick = TdfStruct::new();
            tick.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
            tick.set("PORT", TdfValue::Integer(443));
            tick.set("SKEY", TdfValue::String(String::new()));

            let mut resp = TdfStruct::new();
            resp.set("TELE", TdfValue::Struct(tele));
            resp.set("TICK", TdfValue::Struct(tick));
            Ok(BlazePacket::response(&req.header, resp)?)
        }

        // Util::getTelemetryServer (0x0009:0x0005)
        (0x0009, 0x0005) => {
            logger.log("[Blaze Auth] Handled Util::getTelemetryServer");
            let mut tele = TdfStruct::new();
            tele.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
            tele.set("ANON", TdfValue::Integer(1));
            tele.set("DISA", TdfValue::String(String::new()));
            tele.set("FILT", TdfValue::String(String::new()));
            tele.set("LOC", TdfValue::Integer(0));
            tele.set("NOOK", TdfValue::String(String::new()));
            tele.set("PORT", TdfValue::Integer(443));
            tele.set("SDLY", TdfValue::Integer(0));
            tele.set("SKEY", TdfValue::String(String::new()));
            tele.set("SPCT", TdfValue::Integer(100));
            Ok(BlazePacket::response(&req.header, tele)?)
        }

        // Util::getTickerServer (0x0009:0x001A)
        (0x0009, 0x001A) => {
            logger.log("[Blaze Auth] Handled Util::getTickerServer");
            let mut tick = TdfStruct::new();
            tick.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
            tick.set("PORT", TdfValue::Integer(443));
            tick.set("SKEY", TdfValue::String(String::new()));
            Ok(BlazePacket::response(&req.header, tick)?)
        }

        // Authentication::logout (0x0001:70)
        (0x0001, 70) => {
            logger.log("[Blaze Auth] Handled Authentication::logout -> Success");
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

fn extract_query_param(req_str: &str, key: &str) -> Option<u32> {
    let pattern = format!("{}=", key);
    if let Some(pos) = req_str.find(&pattern) {
        let rest = &req_str[pos + pattern.len()..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return num.parse().ok();
    }
    None
}

fn generate_http_response(req_str: &str) -> Vec<u8> {
    if req_str.contains("getCACertificates") {
        let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<cacertificates>\n</cacertificates>";
        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/xml; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            body.len(),
            body
        ).into_bytes();
    }

    if req_str.contains("redirector") {
        let xml_body = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<serverinstanceinfo>
  <address member="0">
    <valu>
      <hostname>spring18.gosredirector.ea.com</hostname>
      <ip>2130706433</ip>
      <port>42128</port>
      <qosip>2130706433</qosip>
      <qosport>17498</qosport>
    </valu>
  </address>
  <qosip>2130706433</qosip>
  <qosport>17498</qosport>
  <secure>0</secure>
  <msiz>131072</msiz>
  <trialservice>0</trialservice>
  <defaultdnsaddress>0</defaultdnsaddress>
  <instancetype>0</instancetype>
  <servicename>fifa-2022-pc</servicename>
  <servicenames>fifa-2022-pc</servicenames>
  <servers>
    <entry>
      <instance>
        <instancetype>0</instancetype>
        <servicename>fifa-2022-pc</servicename>
        <servicenames>fifa-2022-pc</servicenames>
        <serviceid>1</serviceid>
        <hostname>spring18.gosredirector.ea.com</hostname>
        <ip>2130706433</ip>
        <port>42128</port>
        <srcip>2130706433</srcip>
        <netmask>4294967295</netmask>
        <srcport>42128</srcport>
        <dstport>42128</dstport>
        <qosip>2130706433</qosip>
        <qosport>17498</qosport>
        <addresses>
          <address member="0">
            <valu>
              <hostname>spring18.gosredirector.ea.com</hostname>
              <ip>2130706433</ip>
              <port>42128</port>
              <srcip>2130706433</srcip>
              <netmask>4294967295</netmask>
              <srcport>42128</srcport>
              <dstport>42128</dstport>
              <serviceid>1</serviceid>
              <qosip>2130706433</qosip>
              <qosport>17498</qosport>
            </valu>
          </address>
        </addresses>
      </instance>
    </entry>
  </servers>
</serverinstanceinfo>"#);

        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/xml; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\
X-BLAZE-ERRORCODE: 0\r\n\
X-BLAZE-COMPONENT: redirector\r\n\
X-BLAZE-COMMAND: getServerInstance\r\n\
X-BLAZE-SEQNO: 0\r\n\r\n{}",
            xml_body.len(),
            xml_body
        ).into_bytes();
    }

    if req_str.contains("/qos/qos") {
        let rqid = extract_query_param(req_str, "rqid").unwrap_or(1);
        let rqsc = extract_query_param(req_str, "rqsc").unwrap_or(0);
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <qos>\n\
               <qosip>2130706433</qosip>\n\
               <qosport>17498</qosport>\n\
               <numprobes>2</numprobes>\n\
               <probesize>1200</probesize>\n\
               <requestid>{}</requestid>\n\
               <reqsecret>{}</reqsecret>\n\
             </qos>",
            rqid, rqsc
        );
        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/xml; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            body.len(),
            body
        ).into_bytes();
    }

    if req_str.contains("/qos/firewall") {
        let rqid = extract_query_param(req_str, "rqid").unwrap_or(1);
        let rqsc = extract_query_param(req_str, "rqsc").unwrap_or(0);
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <firewall>\n\
               <numinterfaces>1</numinterfaces>\n\
               <ips>\n\
                 <ips>2130706433</ips>\n\
               </ips>\n\
               <ports>\n\
                 <ports>17498</ports>\n\
               </ports>\n\
               <requestid>{}</requestid>\n\
               <reqsecret>{}</reqsecret>\n\
             </firewall>",
            rqid, rqsc
        );
        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/xml; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            body.len(),
            body
        ).into_bytes();
    }

    if req_str.contains("/qos/firetype") {
        let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<firetype>\n  <firetype>0</firetype>\n</firetype>";
        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/xml; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            body.len(),
            body
        ).into_bytes();
    }

    if req_str.contains("/getPeerAddress") {
        let body = "address=127.0.0.1\r\nport=3659\r\n";
        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            body.len(),
            body
        ).into_bytes();
    }

    let req_lower = req_str.to_ascii_lowercase();
    if req_lower.contains("token") || req_lower.contains("auth") || req_lower.contains("login") || req_lower.contains("identity") {
        let body = r#"{"access_token":"fifa22_blaze_session_token_12345","token_type":"Bearer","expires_in":31536000,"id_token":"mock_id_token","pid_id":76561198956360709,"persona_id":76561198956360709,"status":"SUCCESS"}"#;
        return format!(
            "HTTP/1.1 200 OK\r\n\
Content-Type: application/json; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
            body.len(),
            body
        ).into_bytes();
    }

    // Default fast-path telemetry/events/bugsentry response:
    let body = "{}";
    format!(
        "HTTP/1.1 200 OK\r\n\
Content-Type: application/json; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
        body.len(),
        body
    ).into_bytes()
}
