import xos


def pick_system_audio_input():
    devices = xos.audio.get_input_devices()
    for i, d in enumerate(devices):
        label = str(d.get("label", "")).lower()
        name = str(d.get("name", "")).lower()
        if "system audio" in label or "loopback" in label or "stereo mix" in name:
            return i
    return None


def committed_sentences_at_none(events):
    """
    Rust contract: each commit is Some(canonical) immediately followed by None.
    Return those canonical strings in order (no word-level stitching).
    """
    out = []
    prev = None
    for e in events:
        if e is None:
            if isinstance(prev, str) and prev.strip():
                out.append(prev.strip())
            prev = None
        else:
            prev = e
    return out


def escape_minecraft_ampersands(text):
    return text.replace("&", "&&")


def print_final_summary(events, last_draft):
    commits = committed_sentences_at_none(events)
    print()
    print(f"--- final joined at None boundaries ({len(commits)} commits) ---")
    if commits:
        joined = " ".join(commits)
        xos.color_print("&3" + escape_minecraft_ampersands(joined) + "&r")
    else:
        xos.color_print("&3(no commits — no None-terminated segments)&r")
    if isinstance(last_draft, str) and last_draft.strip():
        tail = last_draft.strip()
        if not commits or commits[-1].lower() != tail.lower():
            print()
            print("--- open tail (no trailing None yet; not part of cyan join) ---")
            print(tail)
    print("--- end ---")


audio = xos.audio.Microphone(device_id=pick_system_audio_input(), buffer_duration=10.0)
transcriber = xos.audio.transcription(audio, size="tiny")
event_log = []
last_statement = None

try:
    for statement in transcriber.iterate():
        event_log.append(statement)
        print(statement)
        last_statement = statement
except KeyboardInterrupt:
    pass
finally:
    print_final_summary(event_log, last_statement)
