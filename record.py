import xos


audio = xos.audio.system(buffer_duration=10.0)
recorder1 = xos.audio.recording(audio, "test1.mp3")
recorder2 = xos.audio.recording(audio, "test2.mp3")

try:
    while True:
        recorder1.record(wait=False)
        recorder2.record(wait=False)
        xos.sleep(0.02)
except KeyboardInterrupt:
    pass
finally:
    recorder1.finish()
    recorder2.finish()
