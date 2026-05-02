# Study: Japanese vocab — chat-style layout, OSK via double-tap on the composer (mutable).
import xos

VOCAB_COL = "Vocab-expression"
KANA_COL = "Vocab-kana"
MEANING_COL = "Vocab-meaning"
ENGLISH_SENTENCE_COL = "Sentence-meaning"
JAPANESE_SENTENCE_COL = "Sentence-expression"
JAPANESE_KANA_SENTENCE_COL = "Sentence-kana"

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


CLR_BG = (5, 5, 8, 255)
CLR_CARD = (18, 18, 22, 255)
CLR_CARD_EDGE = (45, 48, 58, 255)
CLR_COMPOSER = (28, 28, 34, 255)
CLR_COMPOSER_EDGE = (52, 54, 64, 255)
CLR_MUTED = (130, 132, 145, 255)
CLR_ACCENT = (130, 200, 255, 255)


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
        self.input_focus = False
        self._pick_word()

        self.hero = xos.ui.text(
            "",
            0.08,
            0.065,
            0.92,
            0.15,
            color=xos.color.WHITE,
            hitboxes=False,
            baselines=False,
            font_size=48.0,
        )
        self.caption = xos.ui.text(
            "Japanese · romaji reply",
            0.08,
            0.152,
            0.92,
            0.19,
            color=CLR_MUTED[:3],
            font_size=17.0,
        )
        self.thread_status = xos.ui.text(
            "",
            0.06,
            0.20,
            0.94,
            0.27,
            color=CLR_ACCENT[:3],
            font_size=18.0,
        )
        self.feedback_ui = xos.ui.rich_text(
            "",
            0.05,
            0.28,
            0.95,
            0.88,
            color=xos.color.WHITE,
            font_size=21.0,
            minecraft=True,
            selectable=True,
            mutable=False,
        )
        self.chat_placeholder = xos.ui.text(
            "Message…  romaji",
            0.0,
            0.0,
            0.0,
            0.0,
            color=CLR_MUTED[:3],
            font_size=22.0,
            placeholder="Message…  romaji",
            mutable=False,
            show_cursor=False,
        )
        self.chat_value = xos.ui.text(
            "",
            0.0,
            0.0,
            0.0,
            0.0,
            color=(245, 245, 248),
            font_size=22.0,
            mutable=True,
            show_cursor=True,
            hitboxes=False,
        )

    def _pick_word(self):
        i = int(xos.random.randint(0, self.n - 1))
        self.current = xos.csv.row(self.table, i)

    def _composer_layout(self):
        w, h = self.get_width(), self.get_height()
        scale = float(getattr(self, "xos_scale", 1.0))
        ktop = float(getattr(self, "keyboard_top_y", 1.0))
        margin_px = max(10, int(12 * scale))
        gap_px = max(4, int(6 * scale))
        comp_h = max(44, int(50 * min(scale, 1.12)))
        bottom_px = min(h, int(ktop * h) - gap_px)
        top_px = bottom_px - comp_h
        min_top = int(0.22 * h)
        top_px = max(top_px, min_top)
        ml = margin_px
        mr = w - margin_px
        x1n = ml / w
        x2n = mr / w
        y1n = top_px / h
        y2n = bottom_px / h
        pad_x = max(8, int(10 * scale)) / w
        inner_x1 = x1n + pad_x
        inner_x2 = x2n - pad_x
        inner_y1 = y1n + 0.12 * (y2n - y1n)
        inner_y2 = y2n - 0.08 * (y2n - y1n)
        return {
            "outer": (x1n, y1n, x2n, y2n),
            "inner": (inner_x1, inner_y1, inner_x2, inner_y2),
            "px": (ml, top_px, mr, bottom_px),
        }

    def _build_feedback(self):
        w = self.current
        ja = w.get(JAPANESE_SENTENCE_COL, "")
        kana = w.get(JAPANESE_KANA_SENTENCE_COL, "")
        en = w.get(ENGLISH_SENTENCE_COL, "")
        vk = katakana_to_hiragana(w.get(KANA_COL, ""))
        vword = w.get(VOCAB_COL, "")
        mean = w.get(MEANING_COL, "")
        ok = self.last_correct
        head = "&aCorrect&r\n" if ok else "&cIncorrect&r\n"
        line1 = f"{head}「{vk}」は「{vword}」。（{mean}）"
        parts = [
            line1,
            "",
            ja,
            kana,
            en,
            "",
            "&eDrag to select · Enter continues&r",
        ]
        return "\n".join(parts)

    def _clear_feedback_selection(self):
        self.fb_dragging = False
        self.fb_anchor = None
        self.fb_sel = None

    def on_viewport_double_tap(self, x, y):
        if self.state != "prompt":
            return
        if (
            getattr(self.chat_value, "mutable", False)
            and self.chat_value.contains_pixel(x, y)
        ):
            xos.keyboard.toggle_onscreen()

    def on_mouse_down(self, x, y):
        if self.state == "feedback":
            ix = self.feedback_ui.pick(x, y)
            if ix < 0:
                self._clear_feedback_selection()
                return
            self.fb_anchor = ix
            self.fb_sel = (ix, ix + 1)
            self.fb_dragging = True
            return
        self.input_focus = self.chat_value.contains_pixel(x, y)

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

    def _carets_on(self):
        if not self.chat_value.show_cursor or not self.input_focus:
            return ""
        return "|" if (self.t // 14) % 2 == 0 else ""

    def tick(self):
        self.frame.clear(CLR_BG[:3])
        w, h = self.get_width(), self.get_height()
        xos.rasterizer.rects_filled(
            self.frame,
            int(0.05 * w),
            int(0.04 * h),
            int(0.95 * w),
            int(0.175 * h),
            CLR_CARD,
        )
        # rectangles() expects flat [x1,y1,x2,y2] normalized, not [[pt],[pt]] (see tensor_flat_data_list).
        xos.rasterizer.rectangles(
            self.frame,
            [0.05, 0.04, 0.95, 0.175],
            CLR_CARD_EDGE[:3],
            1.0,
        )

        word = self.current.get(VOCAB_COL, "") if self.current else ""
        self.hero.text = word
        self.hero.render(self.frame)

        self.caption.render(self.frame)

        lay = self._composer_layout()
        ox1, oy1, ox2, oy2 = lay["outer"]
        ix1, iy1, ix2, iy2 = lay["inner"]

        if self.state == "prompt":
            self.feedback_ui.text = ""
            self.thread_status.text = (
                "Double-tap the bar to show the keyboard"
                if not bool(getattr(self, "onscreen_keyboard_visible", False))
                else "Type romaji · Enter sends"
            )
            self.thread_status.render(self.frame)

            xos.rasterizer.rects_filled(
                self.frame,
                int(ox1 * w),
                int(oy1 * h),
                int(ox2 * w),
                int(oy2 * h),
                CLR_COMPOSER,
            )
            xos.rasterizer.rectangles(
                self.frame,
                [ox1, oy1, ox2, oy2],
                CLR_COMPOSER_EDGE[:3],
                1.0,
            )

            self.chat_value.x1 = ix1
            self.chat_value.y1 = iy1
            self.chat_value.x2 = ix2
            self.chat_value.y2 = iy2
            self.chat_placeholder.x1 = ix1
            self.chat_placeholder.y1 = iy1
            self.chat_placeholder.x2 = ix2
            self.chat_placeholder.y2 = iy2

            ph_on = not self.guess_buf and not self.input_focus
            if ph_on:
                self.chat_placeholder.text = self.chat_placeholder.placeholder
                self.chat_value.text = ""
                self.chat_placeholder.render(self.frame)
            else:
                self.chat_value.text = self.guess_buf + self._carets_on()
                self.chat_value.render(self.frame)

            ft_y2 = oy1 - 0.012
            self.feedback_ui.y2 = max(0.28, min(0.92, ft_y2))
        else:
            self.thread_status.text = "Review"
            self.thread_status.render(self.frame)
            self.feedback_ui.text = self.feedback_text
            self.feedback_ui.y2 = 0.93
            lo, hi = (-1, -1)
            if self.fb_sel is not None:
                lo, hi = self.fb_sel
            self.feedback_ui.render(
                self.frame, selection_start=lo, selection_end=hi
            )

    def on_key_char(self, ch):
        if self.state == "feedback":
            if ch == "\n" or ch == "\r":
                self.state = "prompt"
                self.guess_buf = ""
                self.feedback_text = ""
                self._clear_feedback_selection()
                self._pick_word()
            return
        self.input_focus = True
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
            if len(self.guess_buf) < 160:
                self.guess_buf += ch


if __name__ == "__main__":
    StudyApp().run()
