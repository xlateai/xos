//! LAN mesh: mutual RSA signatures + X25519 ECDH, then AES-256-GCM for all payloads.

use xos_auth::{
    load_identity, node_id_from_public_pem, rsa_sign, rsa_verify, UnlockedNodeIdentity,
};
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng as AesOsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use hkdf::Hkdf;
use rsa::pkcs8::DecodePublicKey;
use rsa::RsaPublicKey;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use x25519_dalek::{EphemeralSecret, PublicKey};

const HS_VER: u32 = 3;
const HS_VER_PRE_ACCOUNT: u32 = 2;

fn supported_hs(ver: u32) -> bool {
    ver == HS_VER || ver == HS_VER_PRE_ACCOUNT
}

fn local_account_fingerprint() -> Result<String, String> {
    let account = load_identity().map_err(|e| e.to_string())?;
    node_id_from_public_pem(account.public_pem.as_str()).map_err(|e| e.to_string())
}

fn expect_node_id(nid: &str, pk_pem: &str) -> Result<(), String> {
    let exp = node_id_from_public_pem(pk_pem).map_err(|e| e.to_string())?;
    if exp != nid {
        return Err("LAN handshake: node id does not match public key".to_string());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct HsClientHello {
    hs: u32,
    /// When `Some(true)`, joiner requests the UDP mesh data plane (**must match** coordinator).
    #[serde(default)]
    mesh_udp: Option<bool>,
    /// Friendly machine name.
    nn: String,
    /// Account identity fingerprint (SHA256(SPKI DER) of account public key).
    #[serde(default)]
    aid: Option<String>,
    /// SHA256(SPKI DER) hex of `pk` — redundant but lets peers route before parsing PEM.
    nid: String,
    pk: String,
    ec: String,
    nc: String,
}

#[derive(Serialize, Deserialize)]
struct HsServerHello {
    hs: u32,
    #[serde(default)]
    mesh_udp: Option<bool>,
    nn: String,
    /// Account identity fingerprint (must match client `aid`).
    #[serde(default)]
    aid: Option<String>,
    nid: String,
    pk: String,
    ec: String,
    ns: String,
}

#[derive(Serialize, Deserialize)]
struct HsSig {
    hs: u32,
    sig: String,
}

/// After handshake, both sides hold the same key material; **field meaning depends on role**:
/// - **Host:** `tx` = AES key for **host→peer** (h2p); `rx` = **peer→host** (p2h).
/// - **Client:** `tx` = encrypt **peer→host**; `rx` = decrypt **host→peer** (same h2p/p2h bytes as host).
#[derive(Clone)]
pub struct LanWireKeys {
    pub tx: Aes256Gcm,
    pub rx: Aes256Gcm,
}

fn hkdf_client(shared: &[u8]) -> Result<LanWireKeys, String> {
    let hk = Hkdf::<Sha256>::new(None, shared);
    let mut h2p = [0u8; 32];
    let mut p2h = [0u8; 32];
    hk.expand(b"xos-mesh-h2p-v1", &mut h2p)
        .map_err(|e| e.to_string())?;
    hk.expand(b"xos-mesh-p2h-v1", &mut p2h)
        .map_err(|e| e.to_string())?;
    Ok(LanWireKeys {
        tx: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&p2h)),
        rx: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&h2p)),
    })
}

fn hkdf_server(shared: &[u8]) -> Result<LanWireKeys, String> {
    let hk = Hkdf::<Sha256>::new(None, shared);
    let mut h2p = [0u8; 32];
    let mut p2h = [0u8; 32];
    hk.expand(b"xos-mesh-h2p-v1", &mut h2p)
        .map_err(|e| e.to_string())?;
    hk.expand(b"xos-mesh-p2h-v1", &mut p2h)
        .map_err(|e| e.to_string())?;
    Ok(LanWireKeys {
        tx: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&h2p)),
        rx: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&p2h)),
    })
}

/// Returns wire keys and `(read_half, write_half)` for the TCP connection after handshake.
pub fn client_handshake(
    stream: TcpStream,
    id: &UnlockedNodeIdentity,
    mesh_udp: bool,
) -> Result<(LanWireKeys, BufReader<TcpStream>, TcpStream), String> {
    let aid = local_account_fingerprint()?;
    let mut write_half = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);

    let eph = EphemeralSecret::random_from_rng(&mut AesOsRng);
    let ec_pub = PublicKey::from(&eph);
    let mut nc = [0u8; 32];
    getrandom::fill(&mut nc).map_err(|e| format!("{e:?}"))?;

    let nid = id.node_id();
    let hello = HsClientHello {
        hs: HS_VER,
        mesh_udp: Some(mesh_udp),
        nn: id.node_name.clone(),
        aid: Some(aid.clone()),
        nid,
        pk: id.public_pem.clone(),
        ec: B64.encode(ec_pub.as_bytes()),
        nc: B64.encode(nc),
    };
    let line = serde_json::to_string(&hello).map_err(|e| e.to_string())?;
    write_half
        .write_all(format!("{}\n", line).as_bytes())
        .map_err(|e| e.to_string())?;
    write_half.flush().map_err(|e| e.to_string())?;

    let mut buf = String::new();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;
    if buf.is_empty() {
        return Err("connection closed during LAN handshake".to_string());
    }
    let srv: HsServerHello = serde_json::from_str(buf.trim()).map_err(|e| e.to_string())?;
    if !supported_hs(srv.hs) {
        return Err("LAN handshake: bad server hello".to_string());
    }
    if let Some(peer_aid) = srv.aid.as_deref() {
        if peer_aid != aid {
            return Err("LAN handshake: account identity mismatch (different login)".to_string());
        }
    } else if srv.hs >= HS_VER {
        return Err("LAN handshake: account identity mismatch (different login)".to_string());
    }
    let srv_mesh_udp = srv.mesh_udp.unwrap_or(false);
    if srv_mesh_udp != mesh_udp {
        return Err(if mesh_udp {
            "mesh udp mismatch: joiner requested udp=True but coordinator did not agree (every node must use xos.mesh.connect(..., udp=True))"
                .to_string()
        } else {
            "mesh udp mismatch: coordinator requested udp=True but joiner connected with udp=False"
                .to_string()
        });
    }
    expect_node_id(&srv.nid, &srv.pk)?;
    let ec_s_bytes = B64.decode(&srv.ec).map_err(|e| e.to_string())?;
    let ec_s_arr: [u8; 32] = ec_s_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "bad server ec len".to_string())?;
    let peer_ec = PublicKey::from(ec_s_arr);
    let pk_s = RsaPublicKey::from_public_key_pem(srv.pk.as_str()).map_err(|e| e.to_string())?;

    let shared = eph.diffie_hellman(&peer_ec);

    let ns_bytes = B64.decode(&srv.ns).map_err(|e| e.to_string())?;
    let mut sign_msg = Vec::with_capacity(ns_bytes.len() + 64);
    sign_msg.extend_from_slice(&ns_bytes);
    sign_msg.extend_from_slice(peer_ec.as_bytes());
    sign_msg.extend_from_slice(ec_pub.as_bytes());
    let sig = rsa_sign(id.private_key(), &sign_msg).map_err(|e| e.to_string())?;
    let sig_line = HsSig {
        hs: HS_VER,
        sig: B64.encode(&sig),
    };
    let line = serde_json::to_string(&sig_line).map_err(|e| e.to_string())?;
    write_half
        .write_all(format!("{}\n", line).as_bytes())
        .map_err(|e| e.to_string())?;
    write_half.flush().map_err(|e| e.to_string())?;

    buf.clear();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;
    let srv_sig: HsSig = serde_json::from_str(buf.trim()).map_err(|e| e.to_string())?;
    if !supported_hs(srv_sig.hs) {
        return Err("LAN handshake: bad server sig".to_string());
    }
    let sig_bytes = B64.decode(&srv_sig.sig).map_err(|e| e.to_string())?;
    let mut verify_msg = Vec::with_capacity(nc.len() + 64);
    verify_msg.extend_from_slice(&nc);
    verify_msg.extend_from_slice(ec_pub.as_bytes());
    verify_msg.extend_from_slice(peer_ec.as_bytes());
    rsa_verify(&pk_s, &verify_msg, &sig_bytes).map_err(|e| e.to_string())?;

    let keys = hkdf_client(shared.as_bytes())?;
    Ok((keys, reader, write_half))
}

pub fn server_handshake(
    stream: TcpStream,
    id: &UnlockedNodeIdentity,
    coordinator_mesh_udp: bool,
) -> Result<(LanWireKeys, BufReader<TcpStream>, TcpStream), String> {
    let aid = local_account_fingerprint()?;
    let mut write_half = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);

    let mut buf = String::new();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;
    if buf.is_empty() {
        return Err("connection closed during LAN handshake".to_string());
    }
    let cli: HsClientHello = serde_json::from_str(buf.trim()).map_err(|e| e.to_string())?;
    if !supported_hs(cli.hs) {
        return Err("LAN handshake: bad client hello".to_string());
    }
    if let Some(peer_aid) = cli.aid.as_deref() {
        if peer_aid != aid {
            return Err("LAN handshake: account identity mismatch (different login)".to_string());
        }
    } else if cli.hs >= HS_VER {
        return Err("LAN handshake: account identity mismatch (different login)".to_string());
    }
    let joiner_udp = cli.mesh_udp.unwrap_or(false);
    if joiner_udp != coordinator_mesh_udp {
        return Err(if coordinator_mesh_udp {
            format!(
                "mesh udp mismatch: joiner udp={joiner_udp} but coordinator binds with udp=True (all nodes must use udp=True)"
            )
        } else {
            format!(
                "mesh udp mismatch: joiner udp={joiner_udp} but coordinator binds with udp=False"
            )
        });
    }
    expect_node_id(&cli.nid, &cli.pk)?;
    let pk_c = RsaPublicKey::from_public_key_pem(cli.pk.as_str()).map_err(|e| e.to_string())?;
    let ec_c_bytes = B64.decode(&cli.ec).map_err(|e| e.to_string())?;
    let ec_c_arr: [u8; 32] = ec_c_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "bad client ec len".to_string())?;
    let peer_c = PublicKey::from(ec_c_arr);

    let eph = EphemeralSecret::random_from_rng(&mut AesOsRng);
    let ec_s = PublicKey::from(&eph);
    let mut ns = [0u8; 32];
    getrandom::fill(&mut ns).map_err(|e| format!("{e:?}"))?;

    let hello = HsServerHello {
        hs: HS_VER,
        mesh_udp: Some(coordinator_mesh_udp),
        nn: id.node_name.clone(),
        aid: Some(aid),
        nid: id.node_id(),
        pk: id.public_pem.clone(),
        ec: B64.encode(ec_s.as_bytes()),
        ns: B64.encode(ns),
    };
    let line = serde_json::to_string(&hello).map_err(|e| e.to_string())?;
    write_half
        .write_all(format!("{}\n", line).as_bytes())
        .map_err(|e| e.to_string())?;
    write_half.flush().map_err(|e| e.to_string())?;

    buf.clear();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;
    let cli_sig: HsSig = serde_json::from_str(buf.trim()).map_err(|e| e.to_string())?;
    if !supported_hs(cli_sig.hs) {
        return Err("LAN handshake: bad client sig".to_string());
    }
    let sig_bytes = B64.decode(&cli_sig.sig).map_err(|e| e.to_string())?;
    let mut verify_msg = Vec::with_capacity(ns.len() + 64);
    verify_msg.extend_from_slice(&ns);
    verify_msg.extend_from_slice(ec_s.as_bytes());
    verify_msg.extend_from_slice(peer_c.as_bytes());
    rsa_verify(&pk_c, &verify_msg, &sig_bytes).map_err(|e| e.to_string())?;

    let shared = eph.diffie_hellman(&peer_c);

    let nc_bytes = B64.decode(&cli.nc).map_err(|e| e.to_string())?;
    let mut sign_msg = Vec::with_capacity(nc_bytes.len() + 64);
    sign_msg.extend_from_slice(&nc_bytes);
    sign_msg.extend_from_slice(peer_c.as_bytes());
    sign_msg.extend_from_slice(ec_s.as_bytes());
    let sig = rsa_sign(id.private_key(), &sign_msg).map_err(|e| e.to_string())?;
    let sig_line = HsSig {
        hs: HS_VER,
        sig: B64.encode(&sig),
    };
    let line = serde_json::to_string(&sig_line).map_err(|e| e.to_string())?;
    write_half
        .write_all(format!("{}\n", line).as_bytes())
        .map_err(|e| e.to_string())?;
    write_half.flush().map_err(|e| e.to_string())?;

    let keys = hkdf_server(shared.as_bytes())?;
    Ok((keys, reader, write_half))
}

/// Encrypt inner JSON line (UTF-8) for wire v2.
pub fn encrypt_mesh_line(cipher: &Aes256Gcm, inner: &str) -> Result<String, String> {
    let nonce = Aes256Gcm::generate_nonce(&mut AesOsRng);
    let ct = cipher
        .encrypt(&nonce, inner.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ct);
    let obj = json!({"v": 2, "d": B64.encode(&combined)});
    Ok(format!(
        "{}\n",
        serde_json::to_string(&obj).map_err(|e| e.to_string())?
    ))
}

/// Decrypt wire v2 line to inner UTF-8 JSON.
pub fn decrypt_mesh_line(cipher: &Aes256Gcm, line: &str) -> Result<String, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("empty mesh line".to_string());
    }
    let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    if v.get("v").and_then(|x| x.as_u64()) != Some(2) {
        return Err("expected encrypted mesh line v=2".to_string());
    }
    let d = v
        .get("d")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "missing d".to_string())?;
    let raw = B64.decode(d).map_err(|e| e.to_string())?;
    if raw.len() < 13 {
        return Err("truncated ciphertext".to_string());
    }
    let nonce = Nonce::from_slice(&raw[..12]);
    let plain = cipher.decrypt(nonce, raw[12..].as_ref()).map_err(|e| {
        format!("LAN mesh decrypt failed (wrong AES key or corrupt ciphertext): {e}")
    })?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}

/// Max plaintext bytes per UDP mesh datagram before AES-GCM (fits in one IPv4 UDP payload).
pub const MESH_UDP_PAYLOAD_CHUNK: usize = 48 * 1024;

const MUDP_MAGIC: [u8; 4] = *b"XMU1";
const MUDP_HDR: usize = 4 + 8 + 4 + 4;

/// One AES-GCM datagram: `magic | msg_id | idx | total | nonce | ciphertext`.
pub fn mesh_udp_encrypt_chunk(
    cipher: &Aes256Gcm,
    msg_id: u64,
    idx: u32,
    total: u32,
    plain: &[u8],
) -> Result<Vec<u8>, String> {
    let nonce = Aes256Gcm::generate_nonce(&mut AesOsRng);
    let mut hdr = [0u8; MUDP_HDR];
    hdr[0..4].copy_from_slice(&MUDP_MAGIC);
    hdr[4..12].copy_from_slice(&msg_id.to_le_bytes());
    hdr[12..16].copy_from_slice(&idx.to_le_bytes());
    hdr[16..20].copy_from_slice(&total.to_le_bytes());
    let ct = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plain,
                aad: &hdr,
            },
        )
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(MUDP_HDR + 12 + ct.len());
    out.extend_from_slice(&hdr);
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Returns `(msg_id, idx, total, plaintext_chunk)` or error if not a valid mesh UDP frame.
pub fn mesh_udp_try_decrypt_chunk(
    cipher: &Aes256Gcm,
    buf: &[u8],
) -> Result<(u64, u32, u32, Vec<u8>), String> {
    if buf.len() < MUDP_HDR + 12 + 16 {
        return Err("short mesh UDP packet".to_string());
    }
    if buf[0..4] != MUDP_MAGIC {
        return Err("bad mesh UDP magic".to_string());
    }
    let hdr = &buf[0..MUDP_HDR];
    let msg_id = u64::from_le_bytes(buf[4..12].try_into().map_err(|_| "msg_id")?);
    let idx = u32::from_le_bytes(buf[12..16].try_into().map_err(|_| "idx")?);
    let total = u32::from_le_bytes(buf[16..20].try_into().map_err(|_| "total")?);
    if total == 0 || idx >= total {
        return Err("bad mesh UDP fragment indices".to_string());
    }
    let nonce = Nonce::from_slice(&buf[20..32]);
    let ct = &buf[32..];
    let plain = cipher
        .decrypt(
            nonce,
            Payload {
                msg: ct,
                aad: hdr,
            },
        )
        .map_err(|e| format!("mesh UDP decrypt: {e}"))?;
    Ok((msg_id, idx, total, plain))
}

/// Split a full wire JSON line into AES-GCM UDP datagrams sharing `msg_id`.
pub fn mesh_udp_encrypt_inner(cipher: &Aes256Gcm, inner: &str) -> Result<Vec<Vec<u8>>, String> {
    let b = inner.as_bytes();
    let n = b.len();
    let n_chunks = n.div_ceil(MESH_UDP_PAYLOAD_CHUNK);
    let total = n_chunks.max(1) as u32;
    let mut msg_id = [0u8; 8];
    getrandom::fill(&mut msg_id).map_err(|e| format!("{e:?}"))?;
    let msg_id = u64::from_le_bytes(msg_id);
    let mut out = Vec::with_capacity(total as usize);
    for i in 0..total {
        let start = (i as usize) * MESH_UDP_PAYLOAD_CHUNK;
        let end = (start + MESH_UDP_PAYLOAD_CHUNK).min(n);
        let chunk = if start < n { &b[start..end] } else { &[] };
        out.push(mesh_udp_encrypt_chunk(cipher, msg_id, i, total, chunk)?);
    }
    Ok(out)
}
