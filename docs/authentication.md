# 🔗 Mesh & identity

Most software still assumes **one program is the authority** and everything else phones home. xos turns that around: **cooperation is the default**, and “how far the signal travels” is a dial you turn—not a different product category every time.

Mesh is the name we give to **a small graph of peers** that chose the same **connection id**—a shared name for *this* collaboration, not a server you rent. Think of it less like configuring a VPC and more like **naming the room** so the right machines find each other.

---

## ℹ️ Why meshes first

We want **local**, **LAN**, and (eventually) **online** to feel like the **same idea** at different radii. Your app shouldn’t become a different architecture when you leave localhost—only the **scope** changes. That’s the step-up: start tight, widen when you need to, without rewriting your mental model.

---

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
2. Choose a **username** and **password** you’ll remember.
3. Run something like **`xos app mesh`** and connect with **`mode="lan"`**.

If you skip step 1–2, LAN mode will nudge you to log in first. That’s intentional: we’d rather fail fast than pretend trust exists.

---

## 🔑 Username & password on the wire

Once identity exists, **LAN** still asks you to **unlock** with that username and password when a process needs your keys (you might see a prompt per run, or pass **`password=`** where the API allows). New process, fresh unlock—unless later you wire in OS-level credential storage. The point: **your secret stays yours**; we’re not silently caching passwords in this flow.

---

## 🧬 How keys are born (v2)

New identities don’t stash a private key blob or your password on disk.

- Your password is **stretched** with **Argon2id**, using a **salt tied to your username**—not a random salt guarding a random key file, but a **deterministic recipe** you can re-run anywhere.
- That yields a **seed**; from it we generate **RSA-2048** the same way every time.
- We only persist **username**, KDF settings, and your **public** key. The **private** key is **recomputed** when you unlock—no password file, no duplicated secret material.

**Same username + password → the same key pair on any device.** You get a **shared cryptographic identity** across machines without emailing a `.pem` around: you share the *human* secret, not a file.

Older installs may still use the legacy encrypted format (`xos-auth-v1`); new **`xos login --offline`** registrations use **v2**.

---

## 🧰 Where the real code lives

Wire formats and crypto live in Rust (`core::auth`, LAN handshake, mesh runtime)—this page is the **why**, not the **spec**.
