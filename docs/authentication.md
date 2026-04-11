# 🔗 Mesh & identity

## 🎯 Three scopes

- **`local`** — Same machine. Fast iteration, no network story to think about.
- **`lan`** — The room extends to your network: nearby devices can join the same named mesh without you standing up infra.
- **`online`** — Not here yet; reserved for when the same pattern reaches past the LAN.

---

## 🏠 No “mesh server” in the closet

There isn’t a separate xos service to deploy or babysit. **Whoever shows up first** for a given connection id can coordinate; others attach. Discovery does the boring work so you’re not typing IP addresses. If the coordinator goes away, the next run can pick up the pattern—same name, same place, new moment.

---

## 🏷️ Connection id (`mesh_id`)

Every mesh has a **string id**—the handle for “this session, this project, this experiment.” It’s the same instinct as naming a **Weights & Biases** project or run group so everyone lands in the **right story**, except we keep it **OS-native**: plain strings, local identity, no account wall for LAN work.

---

## 🔐 Before you try LAN: `xos login`

LAN mesh uses **cryptographic identity** so peers know who they’re talking to. You register once on the machine:

1. **`xos login --offline`** — no cloud OAuth required; good for air-gapped or “just let me work” setups.
2. Choose a **username** and **password** you’ll remember (password is used **once** to derive your RSA key pair; it is **not** stored).
3. Run something like **`xos app mesh`** and connect with **`mode="lan"`** — the app loads your **private key from disk** (same machine, same `identity.json`). No password prompt on connect.

If you skip step 1–2, LAN mode will nudge you to log in first. That’s intentional: we’d rather fail fast than pretend trust exists.

---

## 🔑 What’s on disk (v4, current)

- **Private + public key:** PEM in **`identity.json`** (PKCS#8 private + SPKI public). Derived once at **`xos login --offline`** from username + password (Argon2id + deterministic RSA). **Password is not stored** — not in the file, not in the OS credential store.
- **LAN mesh** uses **`load_identity()`**: reads the PEM from disk only. Protect **`identity.json`** like any secret (e.g. `0600` on Unix).

Legacy **`xos-auth-v3`** (encrypted PKCS#8), **`xos-auth-v2`** (derive-only), and **`xos-auth-v1`** may still exist on disk; LAN mesh expects **v4**. To migrate: **`xos login --delete`** then **`xos login --offline`**.

---

## 🧰 Where the real code lives

Wire formats and crypto live in Rust (`core::auth`, LAN handshake, mesh runtime)—this page is the **why**, not the **spec**.
