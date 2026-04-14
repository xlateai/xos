import xos


audio = xos.audio.system(buffer_duration=10.0)
transcriber = xos.audio.transcription(audio, size="tiny")
recorder = xos.audio.recording(audio, "test.mp3")

try:
    while True:
        # let the buffer accrue a bit before processing
        xos.sleep(0.02)

        # record the audio
        recorder.record(wait=False)
        
        # transcribe the audio
        transcription, was_committed, is_new = transcriber.transcribe()
        if is_new:
            color = "&a" if was_committed else "&8"
            xos.print_color(color + transcription)

except KeyboardInterrupt:
    pass
finally:
    recorder.finish()
