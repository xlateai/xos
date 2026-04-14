import xos


audio = xos.audio.system(buffer_duration=10.0)
transcriber = xos.audio.transcription(audio, size="tiny")
recorder = xos.audio.recording(audio, "test.mp3")

full_transcription = []

try:
    while True:
        # let the buffer accrue a bit before processing
        xos.sleep(0.02)

        # record the audio
        recorder.record(wait=False)
        
        # transcribe the audio
        transcription, was_committed, is_new = transcriber.transcribe()
        if is_new:
            if was_committed:
                xos.print_color("&a" + transcription)
                full_transcription.append(transcription)
            else:
                xos.print_color("[*] &8" + transcription)

except KeyboardInterrupt:
    pass
finally:
    print("----------- FINAL TRANSCRIPTION -----------")
    print("\n".join(full_transcription))
    print("-------------------------------------------")
    recorder.finish()
    transcriber.finish()
