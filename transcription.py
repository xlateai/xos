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
transcriber = xos.audio.transcription(audio)
committed_statements = []
last_statement = None
printed_live = ""

for statement in transcriber.iterate():
    print(statement)

    if statement is None:
        if last_statement:
            committed_statements.append(last_statement)
            # xos.print(last_statement)
            # printed_live = ""
        # last_statement = None
        # continue

    # Live draft update (same line, no commit yet)
    # if statement != printed_live:
        # xos.print("\r" + statement, end="")
        # printed_live = statement


    last_statement = statement
