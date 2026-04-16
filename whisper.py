import xos

# load the whisper model
whisper = xos.ai.whisper.load("tiny")

# let's us look directly into all of the parameters like you can with pytorch
for i, (name, param) in enumerate(whisper.named_parameters()):
    # print stats of the layer
    print(f"{i}: {name}: {param.shape}")
    print(f"Stats: mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")


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


print("-----------------------------------------------")
print("forward function with layer-wise statistics:")
for (layer_name, output) in whisper.forward_layer_by_layer(x):
    # if layer_name is None then it is the output (instead of activation)
    # print the layer name and shape of the activation as well as the shape of the parameters
    if layer_name is not None:
        param = whisper.get_parameter(layer_name)
        # print(f"{layer_name}: {param.shape} --param produces--> {output.shape}")
        # print param name and stats
        print(f"--------{layer_name}--------")
        print(f"Param Stats: shape={param.shape}, mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")
        print(f"Output Stats: shape={output.shape}, mean={output.mean():.4f}, std={output.std():.4f}, min={output.min():.4f}, max={output.max():.4f}")
    else:
        print("output transcripts:", output)

print("-----------------------------------------------")