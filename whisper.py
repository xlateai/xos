import xos

# load the whisper model
whisper = xos.ai.whisper.load("tiny")


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


# inference on random value waveform
print("-----------------------------------------------")
x = xos.random.uniform(1, 1000, shape=(1, 1000))
print(x)
y = whisper.forward(x)
print("random value waveform output transcripts:", y)
print("-----------------------------------------------")


# inference on a real audio waveform
x = xos.audio.load("intro.mp3")
y = whisper.forward(x)
print("intro.mp3 output transcripts:", y)
print(x)


print("-----------------------------------------------")
print("forward function with layer-wise statistics:")
for (layer_name, output) in whisper.forward_layer_by_layer(x):
    # if layer_name is None then it is the output (instead of activation)
    # print the layer name and shape of the activation as well as the shape of the parameters
    if layer_name is not None:
        param = whisper.get_parameter(layer_name)
        print(f"--------{layer_name}--------")
        if param is not None:
            print(f"Param Stats: shape={param.shape}, mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")
        values = output.values if hasattr(output, "values") else []
        finite = sum(1 for v in values if isinstance(v, float) and v == v and abs(v) != float("inf"))
        zero = sum(1 for v in values if v == 0.0)
        zero_ratio = (zero / len(values)) if values else 0.0
        print(f"Output Stats: shape={output.shape}, mean={output.mean():.4f}, std={output.std():.4f}, min={output.min():.4f}, max={output.max():.4f}")
        print(f"Sample Stats: sampled={len(values)}, finite={finite}, zero={zero}, zero_ratio={zero_ratio:.4f}")
    else:
        print("output transcripts:", output)

print("-----------------------------------------------")