import xos

# Same defaults as xos.ai.whisper.load: tiny + CT2.
audio = xos.audio.system(buffer_duration=10.0)
transcriber = xos.audio.transcription(audio, size="tiny")
if xos.flags.record:
    recorder = xos.audio.recording(audio, "test.mp3")

full_transcription = []

try:
    while True:
        # Short yield so the audio thread can fill the ring without spinning the CPU at 100%.
        xos.sleep(0.004)

        if xos.flags.record:
            recorder.record(wait=False)

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
    if xos.flags.record:
        recorder.finish()
    transcriber.finish()
