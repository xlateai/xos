import xos

audio = xos.audio.system(buffer_duration=10.0)
recorder = xos.audio.recording(audio, "test.mp3")
recorder.record()