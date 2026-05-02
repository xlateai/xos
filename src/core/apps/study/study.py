# Study: Japanese vocab guessing (viewport). CSV is auto-downloaded to xos path --data / data / study /
# Random: use xos.random (uniform, randint, choice, uniform_fill) — not stdlib random.
import xos

VOCAB_COL = "Vocab-expression"
KANA_COL = "Vocab-kana"
MEANING_COL = "Vocab-meaning"
ENGLISH_SENTENCE_COL = "Sentence-meaning"
JAPANESE_SENTENCE_COL = "Sentence-expression"
JAPANESE_KANA_SENTENCE_COL = "Sentence-kana"

# --- Romaji <-> Hiragana (subset from xlate dashboards/utils.py) ---
HIRAGANA_MAP = {
    "-": "ー",
    "aa": "ああ",
    "a": "あ",
    "i": "い",
    "u": "う",
    "e": "え",
    "o": "お",
    "oo": "おお",
    "ka": "か",
    "ki": "き",
    "ku": "く",
    "ke": "け",
    "ko": "こ",
    "sa": "さ",
    "shi": "し",
    "su": "す",
    "se": "せ",
    "so": "そ",
    "ta": "た",
    "chi": "ち",
    "tsu": "つ",
    "te": "て",
    "to": "と",
    "ti": "てぃ",
    "fi": "ふぃ",
    "na": "な",
    "ni": "に",
    "nu": "ぬ",
    "ne": "ね",
    "no": "の",
    "ha": "は",
    "hi": "ひ",
    "fu": "ふ",
    "he": "へ",
    "ho": "ほ",
    "ma": "ま",
    "mi": "み",
    "mu": "む",
    "me": "め",
    "mo": "も",
    "ya": "や",
    "yu": "ゆ",
    "yo": "よ",
    "ra": "ら",
    "ri": "り",
    "ru": "る",
    "re": "れ",
    "ro": "ろ",
    "wa": "わ",
    "wo": "を",
    "n": "ん",
    "ga": "が",
    "gi": "ぎ",
    "gu": "ぐ",
    "ge": "げ",
    "go": "ご",
    "za": "ざ",
    "ji": "じ",
    "zu": "ず",
    "ze": "ぜ",
    "zo": "ぞ",
    "da": "だ",
    "ji_d": "ぢ",
    "zu_d": "づ",
    "de": "で",
    "do": "ど",
    "ba": "ば",
    "bi": "び",
    "bu": "ぶ",
    "be": "べ",
    "bo": "ぼ",
    "pa": "ぱ",
    "pi": "ぴ",
    "pu": "ぷ",
    "pe": "ぺ",
    "po": "ぽ",
    "kya": "きゃ",
    "kyu": "きゅ",
    "kyo": "きょ",
    "sha": "しゃ",
    "shu": "しゅ",
    "sho": "しょ",
    "cha": "ちゃ",
    "chu": "ちゅ",
    "cho": "ちょ",
    "nya": "にゃ",
    "nyu": "にゅ",
    "nyo": "にょ",
    "hya": "ひゃ",
    "hyu": "ひゅ",
    "hyo": "ひょ",
    "mya": "みゃ",
    "myu": "みゅ",
    "myo": "みょ",
    "rya": "りゃ",
    "ryu": "りゅ",
    "ryo": "りょ",
    "gya": "ぎゃ",
    "gyu": "ぎゅ",
    "gyo": "ぎょ",
    "ja": "じゃ",
    "ju": "じゅ",
    "jo": "じょ",
    "bya": "びゃ",
    "byu": "びゅ",
    "byo": "びょ",
    "pya": "ぴゃ",
    "pyu": "ぴゅ",
    "pyo": "ぴょ",
    "fui": "ふぃ",
    "fa": "ふぁ",
    "fe": "ふぇ",
    "fo": "ふぉ",
}


def katakana_to_hiragana(s):
    out = []
    for ch in s:
        o = ord(ch)
        if 0x30A0 <= o <= 0x30FF:
            if ch == "ー":
                out.append(ch)
            else:
                out.append(chr(o - 0x60))
        else:
            out.append(ch)
    return "".join(out)


def romaji_to_hiragana(romaji):
    romaji = romaji.lower()
    keys = sorted(HIRAGANA_MAP.keys(), key=lambda x: len(x), reverse=True)
    hiragana = ""
    i = 0
    while i < len(romaji):
        if i < len(romaji) - 1 and romaji[i] == romaji[i + 1] and romaji[i] not in ("n", "o"):
            hiragana += "っ"
            i += 1
            continue
        matched = False
        for key in keys:
            if romaji[i:].startswith(key):
                if key == "ji" and romaji[i - 1 : i] in ("d",):
                    hiragana += HIRAGANA_MAP["ji_d"]
                else:
                    hiragana += HIRAGANA_MAP[key]
                i += len(key)
                matched = True
                break
        if not matched:
            hiragana += romaji[i]
            i += 1
    return hiragana


class StudyApp(xos.Application):
    def __init__(self):
        super().__init__()
        base = xos.path.data()
        path = base + "/data/study/japanese_vocabs_6000.csv"
        self.table = xos.csv.load(path)
        self.n = xos.csv.len(self.table)
        if self.n < 1:
            raise RuntimeError("empty vocabulary CSV")
        self.state = "prompt"
        self.guess_buf = ""
        self.current = None
        self.feedback_text = ""
        self.last_correct = False
        self.fb_dragging = False
        self.fb_anchor = None
        self.fb_sel = None
        self._pick_word()

        self.title = xos.ui.text(
            "",
            0.05,
            0.10,
            0.95,
            0.38,
            color=xos.color.WHITE,
            font_size=56.0,
        )
        self.hint = xos.ui.text(
            "Type romaji, Enter to submit. Backspace edits. Esc clears.",
            0.05,
            0.41,
            0.95,
            0.49,
            color=(180, 180, 200),
            font_size=20.0,
        )
        self.input_label = xos.ui.text(
            "",
            0.05,
            0.51,
            0.95,
            0.59,
            color=(120, 220, 255),
            font_size=28.0,
        )
        self.feedback_ui = xos.ui.rich_text(
            "",
            0.05,
            0.52,
            0.95,
            0.93,
            color=xos.color.WHITE,
            font_size=22.0,
            minecraft=True,
            selectable=True,
        )

    def _pick_word(self):
        # xos.random.randint(a, b) is inclusive on both ends (like Python random.randint).
        i = int(xos.random.randint(0, self.n - 1))
        self.current = xos.csv.row(self.table, i)

    def _build_feedback(self):
        w = self.current
        ja = w.get(JAPANESE_SENTENCE_COL, "")
        kana = w.get(JAPANESE_KANA_SENTENCE_COL, "")
        en = w.get(ENGLISH_SENTENCE_COL, "")
        vk = katakana_to_hiragana(w.get(KANA_COL, ""))
        vword = w.get(VOCAB_COL, "")
        mean = w.get(MEANING_COL, "")
        ok = self.last_correct
        head = "&aCorrect!&r\n" if ok else "&cIncorrect.&r\n"
        line1 = f"{head}「{vk}」は「{vword}」。（{mean}）"
        parts = [
            line1,
            "",
            ja,
            kana,
            en,
            "",
            "&eDrag to highlight • Enter continues&r",
        ]
        return "\n".join(parts)

    def _clear_feedback_selection(self):
        self.fb_dragging = False
        self.fb_anchor = None
        self.fb_sel = None

    def on_mouse_down(self, x, y):
        if self.state != "feedback":
            return
        ix = self.feedback_ui.pick(x, y)
        if ix < 0:
            self._clear_feedback_selection()
            return
        self.fb_anchor = ix
        self.fb_sel = (ix, ix + 1)
        self.fb_dragging = True

    def on_mouse_move(self, x, y):
        if self.state != "feedback" or not self.fb_dragging or self.fb_anchor is None:
            return
        ix = self.feedback_ui.pick(x, y)
        if ix < 0:
            return
        a = self.fb_anchor
        lo = ix if ix < a else a
        hi = (a if ix < a else ix) + 1
        self.fb_sel = (lo, hi)

    def on_mouse_up(self, _x, _y):
        self.fb_dragging = False

    def on_key_char(self, ch):
        if self.state == "feedback":
            if ch == "\n" or ch == "\r":
                self.state = "prompt"
                self.guess_buf = ""
                self.feedback_text = ""
                self._clear_feedback_selection()
                self._pick_word()
            return

        if ch in ("\r", "\n"):
            self._submit_guess()
            return
        if ch == "\x1b":
            self.guess_buf = ""
            return
        if ch == "\b" or ch == "\x7f":
            if len(self.guess_buf) > 0:
                self.guess_buf = self.guess_buf[:-1]
            return
        if len(ch) == 1 and ord(ch) >= 32:
            if len(self.guess_buf) < 120:
                self.guess_buf += ch

    def _submit_guess(self):
        w = self.current
        if w is None:
            return
        raw = self.guess_buf.strip().lower()
        if raw == "exit":
            xos.system.exit(0)
        kana_tgt = katakana_to_hiragana(w.get(KANA_COL, ""))
        guess_hi = romaji_to_hiragana(raw)
        self.last_correct = guess_hi == kana_tgt
        self.state = "feedback"
        self.feedback_text = self._build_feedback()
        self._clear_feedback_selection()

    def tick(self):
        self.frame.clear(xos.color.BLACK)
        if self.state == "prompt":
            w = self.current
            word = w.get(VOCAB_COL, "") if w else ""
            self.title.text = f"Kanji: {word}"
            self.hint.text = "Type romaji, Enter submits. Esc clears • Backspace edits."
            self.input_label.text = f"Your guess (romaji): {self.guess_buf}_"
            self.feedback_ui.text = ""
        else:
            w = self.current
            word = w.get(VOCAB_COL, "") if w else ""
            self.title.text = f"Kanji: {word}"
            self.hint.text = "Drag anywhere on the passage to highlight. Enter → next word."
            self.input_label.text = ""
            self.feedback_ui.text = self.feedback_text

        self.title.render(self.frame)
        if self.state == "prompt":
            self.hint.render(self.frame)
            self.input_label.render(self.frame)
        else:
            self.hint.render(self.frame)
            lo, hi = (-1, -1)
            if self.fb_sel is not None:
                lo, hi = self.fb_sel
            self.feedback_ui.render(
                self.frame, selection_start=lo, selection_end=hi
            )


if __name__ == "__main__":
    StudyApp().run()
