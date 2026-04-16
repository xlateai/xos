import xos

# load the whisper model
whisper = xos.ai.whisper.load("tiny")

# let's us look directly into all of the parameters like you can with pytorch
for i, (name, param) in enumerate(whisper.named_parameters()):
    # print stats of the layer
    print(f"{i}: {name}: {param.shape}")
    print(f"Stats: mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")

# random values to test inference
# x = xos.random.randint(1, 1000)
x = xos.audio.load("intro.mp3")
y = whisper.forward(x)
print(y)