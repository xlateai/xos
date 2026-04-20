import xos

"""
Live Whisper + Silero VAD segmentation (same behavior style as `xos app transcribe`).
"""

# LANGUAGE = "english"  # "english" or "japanese"
LANGUAGE = "japanese"  # "english" or "japanese"

audio = xos.audio.system(buffer_duration=10.0)
transcriber = xos.audio.transcription(audio, size="tiny", language=LANGUAGE)

THRESHOLD = 0.30
SPEECH_START_FRAMES = 2
SILENCE_COMMIT_FRAMES = 5
SILENCE_CLIP_FRAMES = 3
EMA_ALPHA = 0.30

full_transcription = []
segment_live_text = ""
speech_run_frames = 0
silence_run_frames = 0
silence_idle_clip_frames = 0
in_speech_segment = False
vad_ema = 0.0


def normalize_text(s):
    return " ".join(s.split()).strip()


def render():
    print("\x1b[2J\x1b[H", end="")
    if full_transcription:
        xos.print_color("\n".join(full_transcription[-8:]))
    if in_speech_segment and segment_live_text.strip():
        xos.print_color("&8LIVE: " + segment_live_text.strip())
    raw = transcriber.vad_prob()
    xos.print_color(
        f"&7RAW {raw*100:5.1f}%  EMA {vad_ema*100:5.1f}%  THR {THRESHOLD*100:5.1f}%  BUF {transcriber.buffered_seconds():.2f}s"
    )


try:
    while True:
        xos.sleep(0.01)
        transcription, _, is_new = transcriber.transcribe()
        raw = transcriber.vad_prob()
        vad_ema = vad_ema * (1.0 - EMA_ALPHA) + raw * EMA_ALPHA
        seg_end = max(0.01, THRESHOLD * 0.80)
        speech_now = raw >= THRESHOLD or vad_ema >= seg_end

        if speech_now:
            speech_run_frames += 1
            silence_run_frames = 0
            silence_idle_clip_frames = 0
            if speech_run_frames >= SPEECH_START_FRAMES:
                in_speech_segment = True
            if in_speech_segment and is_new and transcription.strip():
                if len(transcription) >= len(segment_live_text):
                    segment_live_text = transcription
        else:
            silence_run_frames += 1
            speech_run_frames = 0
            if in_speech_segment and silence_run_frames >= SILENCE_COMMIT_FRAMES:
                commits = transcriber.flush_commit()
                finalized = ""
                for line in commits:
                    t = normalize_text(line)
                    if t:
                        finalized = t
                live_norm = normalize_text(segment_live_text)
                if not finalized or (live_norm and len(live_norm) > len(finalized)):
                    finalized = live_norm
                if finalized and (not full_transcription or full_transcription[-1] != finalized):
                    full_transcription.append(finalized)
                segment_live_text = ""
                in_speech_segment = False
                transcriber.clip_cursor()
                silence_idle_clip_frames = 0
            elif not in_speech_segment:
                silence_idle_clip_frames += 1
                if silence_idle_clip_frames >= SILENCE_CLIP_FRAMES:
                    transcriber.clip_cursor()
                    silence_idle_clip_frames = 0
        render()
except KeyboardInterrupt:
    pass
finally:
    print("----------- FINAL TRANSCRIPTION -----------")
    print("\n".join(full_transcription))
    print("-------------------------------------------")
    transcriber.finish()