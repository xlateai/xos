import xos


audio = xos.audio.system(buffer_duration=10.0)
recorder = xos.audio.recording(audio, "test.mp3")

try:
    recorder.record()
except KeyboardInterrupt:
    pass
finally:
    recorder.finish()
