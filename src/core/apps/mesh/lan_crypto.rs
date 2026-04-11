//! LAN mesh: mutual RSA signatures + X25519 ECDH, then AES-256-GCM for all payloads.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng as AesOsRng};
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
use crate::auth::{rsa_sign, rsa_verify, UnlockedIdentity};

#[derive(Serialize, Deserialize)]
struct HsClientHello {
    hs: u32,
    u: String,
    pk: String,
    ec: String,
    nc: String,
}

#[derive(Serialize, Deserialize)]
struct HsServerHello {
    hs: u32,
    pk: String,
    ec: String,
    ns: String,
}

#[derive(Serialize, Deserialize)]
struct HsSig {
    hs: u32,
    sig: String,
}

/// Encrypt with `tx`, decrypt with `rx` for this side of the connection.
/// Host→peer payloads use `rx` (h2p); peer→host use `tx` (p2h).
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
    id: &UnlockedIdentity,
) -> Result<(LanWireKeys, BufReader<TcpStream>, TcpStream), String> {
    let mut write_half = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);

    let eph = EphemeralSecret::random_from_rng(&mut AesOsRng);
    let ec_pub = PublicKey::from(&eph);
    let mut nc = [0u8; 32];
    getrandom::fill(&mut nc).map_err(|e| format!("{e:?}"))?;

    let hello = HsClientHello {
        hs: 1,
        u: id.username.clone(),
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
    if srv.hs != 1 {
        return Err("LAN handshake: bad server hello".to_string());
    }
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
        hs: 1,
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
    id: &UnlockedIdentity,
) -> Result<(LanWireKeys, BufReader<TcpStream>, TcpStream), String> {
    let mut write_half = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);

    let mut buf = String::new();
    reader.read_line(&mut buf).map_err(|e| e.to_string())?;
    if buf.is_empty() {
        return Err("connection closed during LAN handshake".to_string());
    }
    let cli: HsClientHello = serde_json::from_str(buf.trim()).map_err(|e| e.to_string())?;
    if cli.hs != 1 {
        return Err("LAN handshake: bad client hello".to_string());
    }
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
        hs: 1,
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
        hs: 1,
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
    let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
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
    let plain = cipher
        .decrypt(nonce, raw[12..].as_ref())
        .map_err(|e| e.to_string())?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}
