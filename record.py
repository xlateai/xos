import xos


def pick_system_audio_input():
    devices = xos.audio.get_input_devices()
    for i, d in enumerate(devices):
        label = str(d.get("label", "")).lower()
        name = str(d.get("name", "")).lower()
        if "system audio" in label or "loopback" in label or "stereo mix" in name:
            return i
    return None


audio = xos.audio.Microphone(device_id=pick_system_audio_input(), buffer_duration=10.0)
recorder = xos.audio.recording(audio, "test.mp3")

try:
    recorder.record()
except KeyboardInterrupt:
    pass
finally:
    recorder.close()
