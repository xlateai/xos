import xos

# load the whisper model
whisper = xos.ai.whisper.load("tiny")

# let's us look directly into all of the parameters like you can with pytorch
for i, (name, param) in enumerate(whisper.named_parameters()):
    # print stats of the layer
    print(f"{i}: {name}: {param.shape}")
    print(f"Stats: mean={param.mean():.4f}, std={param.std():.4f}, min={param.min():.4f}, max={param.max():.4f}")


# standard forward function call
# random values to test inference
# x = xos.random.randint(1, 1000)
x = xos.audio.load("intro.mp3")
y = whisper.forward(x)
print(y)


# forward function call with intermediates
for (param_name, output) in whisper.forward_layer_by_layer(x):
    # if param_name is None then it is the output (instead of activation)
    # print the param name and shape of the activation as well as the shape of the parameters
    if param_name is not None:
        param = whisper.get_parameter(param_name)
        print(f"{param_name}: {output.shape} -> {param.shape}")
    else:
        print("output:")
        print(output)