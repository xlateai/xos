import xos


# KERNEL_BACKEND = xos.ai.whisper.BURN
# KERNEL_BACKEND = xos.ai.whisper.CT2


# load the whisper model
whisper = xos.ai.whisper.load()  # most stable defaults
# whisper = xos.ai.whisper.load("tiny", backend=KERNEL_BACKEND)  # 32 bit
# whisper = xos.ai.whisper.load("tiny-f16", backend=KERNEL_BACKEND)  # 16 bit

def get_color(param_name):
    if "encoder" in param_name:
        return "&3"
    elif "decoder" in param_name:
        return "&4"
    return ""

# let's us look directly into all of the parameters like you can with pytorch
for i, (name, param) in enumerate(whisper.named_parameters()):
    color = get_color(name)
    # print stats of the layer
    xos.print_color(f"{color}{i}: {name}: {param.shape}")
    xos.print_color(f"{color}Stats: mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")


# inference on synthetic audio: must be ~[-1, 1] float PCM at sample_rate (default 16 kHz).
# uniform(1, 1000) is *not* a waveform — it looks like a bug and drives mel/encoder garbage.
# print("-----------------------------------------------")
# sr = 16000
# n = sr * 2  # ~2 s at 16 kHz
# x = xos.random.uniform(-1.0, 1.0, shape=(1, n))
# print(x)
# y = whisper.forward(x, sr)
# print("random value waveform output transcripts:", y)
# print("-----------------------------------------------")


# inference on a real audio waveform (load resamples to `sample_rate`, mono f32)
x = xos.audio.load("intro.mp3", 16000)
y = whisper.forward(x, 16000)
print("intro.mp3 output transcripts:", y)
print(x)


# print("-----------------------------------------------")
# print("forward function with layer-wise statistics:")
# for (layer_name, output) in whisper.forward_layer_by_layer(x):
#     # if layer_name is None then it is the output (instead of activation)
#     # print the layer name and shape of the activation as well as the shape of the parameters
#     if layer_name is not None:
#         param = whisper.get_parameter(layer_name)
#         print(f"--------{layer_name}--------")
#         if param is not None:
#             print(f"Param Stats: shape={param.shape}, mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")
#         values = output.values if hasattr(output, "values") else []
#         st = getattr(output, "stats", {}) or {}
#         n = st.get("num_values", len(values))
#         finite = sum(1 for v in values if isinstance(v, float) and v == v and abs(v) != float("inf"))
#         zero = sum(1 for v in values if v == 0.0)
#         zero_ratio = (zero / len(values)) if values else 0.0
#         print(
#             f"Values stats (num_values={n}): shape={output.shape}, "
#             f"mean={output.mean():.4f}, std={output.std():.4f}, min={output.min():.4f}, max={output.max():.4f}"
#         )
#         print(f"Values: len={len(values)}, finite={finite}, zero={zero}, zero_ratio={zero_ratio:.4f}")
#         if "device_sum" in st or "device_abs_max" in st:
#             ds = st.get("device_sum")
#             dm = st.get("device_abs_max")
#             if ds is not None and dm is not None:
#                 print(
#                     f"GPU preflight (before host readback): sum={float(ds):.6g}, abs_max={float(dm):.6g}"
#                 )
#         if "full_mean" in st:
#             print(
#                 f"Summary (Rust): mean={float(st['full_mean']):.4f}, std={float(st['full_std']):.4f}, "
#                 f"min={float(st['full_min']):.4f}, max={float(st['full_max']):.4f}"
#             )
#     else:
#         print("output transcripts:", output)

# print("-----------------------------------------------")