import xos


DEBUG = True


# Same defaults as xos.ai.whisper.load: tiny + CT2.
audio = xos.audio.system(buffer_duration=10.0)
transcriber = xos.audio.transcription(audio, size="tiny")
if xos.flags.record:
    recorder = xos.audio.recording(audio, "test.mp3")

full_transcription = []
live_transcription = ""


def render_live_paragraph():
    committed = " ".join(full_transcription).strip()
    live = live_transcription.strip()

    if committed and live:
        body = committed + " " + "&8" + live
    elif committed:
        body = committed
    elif live:
        body = "&8" + live
    else:
        body = "&8..."

    # Clear screen and redraw from the top so output feels live.
    print("\x1b[2J\x1b[H", end="")
    xos.print_color(body)


try:
    while True:
        # Short yield so the audio thread can fill the ring without spinning the CPU at 100%.
        xos.sleep(0.004)

        if xos.flags.record:
            recorder.record(wait=False)

        transcription, was_committed, is_new = transcriber.transcribe()

        if DEBUG:
            if is_new:
                if was_committed:
                    xos.print_color("&a" + transcription)
                    full_transcription.append(transcription)
                else:
                    xos.print_color("[*] &8" + transcription)
        else:
            if is_new:
                if was_committed:
                    full_transcription.append(transcription)
                    live_transcription = ""
                else:
                    live_transcription = transcription
                render_live_paragraph()

except KeyboardInterrupt:
    pass
finally:
    if not DEBUG:
        print()
    print("----------- FINAL TRANSCRIPTION -----------")
    print("\n".join(full_transcription))
    print("-------------------------------------------")
    if xos.flags.record:
        recorder.finish()
    transcriber.finish()
