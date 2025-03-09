We need to create a new audio device interface because cpal is limited in functionality. The idea is we should be able to handle all input and output devices the same (for Listeners of course).

Each audio device should have a set of channels that it's responsible for, and the specification for it's sample rate(s).

Each audio device should also have their own buffers per channel. We should be able to scan all channels across all devices at the same time, and the outer waveform software should display all channels belonging to each device and all be colored according to the devices themselves.

# Data

AudioDevice: {
    is_output: boolean,
    buffer: MultiChannelBuffer,
    sample_rate,
    cpal_device,  // for reading the audio to move into the buffer
}

AudioListener: {
    devices: [AudioDevice]
    get_samples() // reads from buffer(s), returning a single matrix across all channels
}