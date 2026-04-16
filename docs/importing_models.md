# Importing Models into xOS

Bring new models into xOS with a simple rule: **great runtime UX + great observability**.  
This guide is intentionally short and practical. 🤝

## What We Support

- Burn-based model runtimes only (for now).
- Python-facing APIs that feel consistent across all model families.

## Two Homes for Every Model

Every model integration needs two Python entry points:

- **Modality home**: where users run the model in workflows.  
Example: `xos.audio` / `xos.audio.transcription`.
- **Raw model home**: where users inspect and call the model directly.  
Example: `xos.ai.whisper` with `xos.ai.whisper.load()`.

Think of it as:

- `xos.<modality>` = product workflow API (ie. xos.audio)
- `xos.ai.<family>` = model lab API (ie. xos.ai.whisper)

## Required Model API (PyTorch-like)

Each loaded model should expose:

- `model.named_parameters()`
- `model.parameters`
- `model.forward(...)`
- `model.forward_layer_by_layer(...)`

This keeps scripts reusable as we expand from Whisper to chat and beyond.

## Parameter + Activation Observability

At minimum, parameters should expose:

- stable names
- shape
- dtype
- useful stats (mean/min/max/std)

And `forward_layer_by_layer(...)` should return intermediate activations in order so developers can debug:

- dtype issues
- NaN/Inf propagation
- bad checkpoints
- unexpected layer behavior

## Module Convention

In Python, model/layer abstractions align with `xos.nn.Module`.  
In Rust, forward-bearing blocks should implement the xOS module trait equivalent.

This gives us:

- recursive module discovery
- automatic forward tracing hooks
- a scalable foundation for future model ports

## Porting Checklist

1. Add Burn model runtime.
2. Add modality API (`xos.<modality>`).
3. Add raw model API (`xos.ai.<family>.load()`).
4. Expose `named_parameters()` + metadata.
5. Ensure forward-bearing layers implement the xOS module convention.
6. Wire `forward_layer_by_layer(...)`.
7. Validate with real samples + debug probes.

## Why This Matters

We are building xOS model integrations with **research-grade visibility** and **production-grade ergonomics**.  
Whisper is first. Chat models are next. Let’s make observability best-in-class. 🚀