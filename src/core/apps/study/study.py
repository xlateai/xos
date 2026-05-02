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


# Guessing-game–style semantics: yellow labels, magenta kanji cue, cyan Japanese, blue English,
# green/red outcomes — bumped saturation on a pure-black stage.
CLR_BG = (0, 0, 0, 255)
CLR_CARD = (38, 16, 52, 255)
CLR_CARD_EDGE = (255, 90, 255, 255)
CLR_COMPOSER = (32, 20, 48, 255)
CLR_COMPOSER_EDGE = (200, 110, 255, 255)
CLR_MUTED = (160, 150, 195, 255)
CLR_THREAD = (115, 255, 255, 255)
CLR_CAPTION = (255, 235, 90, 255)
CLR_KANJI = (255, 118, 255, 255)
CLR_HINT_BAND = (200, 95, 255, 255)

# Viewport text sizes: bases below are multiplied inside Text/RichText.render by `xos.Application.xos_scale`
# (same value as `_study_viewport_ui_coef()` drives for composer geometry below).
FS = 1.2
# Kanji cue line — extra clarity for handwriting along in a notebook (~20 % larger than previous study hero).
HERO_FONT_BASE = 48.0 * FS * 1.2


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

        # Hero band y-ranges; x-range is set each tick from word width (centered in full-bleed card).
        self.hero = xos.ui.text(
            "",
            0.01,
            0.048,
            0.99,
            0.126,
            color=CLR_KANJI[:3],
            hitboxes=False,
            baselines=False,
            font_size=HERO_FONT_BASE,
        )
        self.caption = xos.ui.text(
            "Japanese · romaji reply",
            0.02,
            0.129,
            0.98,
            0.168,
            color=CLR_CAPTION[:3],
            font_size=18.0 * FS,
        )
        self.thread_status = xos.ui.text(
            "",
            0.02,
            0.176,
            0.98,
            0.222,
            color=CLR_THREAD[:3],
            font_size=19.0 * FS,
        )
        self.feedback_ui = xos.ui.rich_text(
            "",
            0.04,
            0.230,
            0.96,
            0.88,
            color=(248, 252, 255),
            font_size=21.0 * FS,
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
            font_size=22.0 * FS,
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
            font_size=22.0 * FS,
            mutable=True,
            show_cursor=True,
            hitboxes=False,
        )
        self.composer_hint = xos.ui.text(
            "",
            0.0,
            0.0,
            0.0,
            0.0,
            color=CLR_MUTED[:3],
            font_size=15.0 * FS,
            mutable=False,
            show_cursor=False,
            hitboxes=False,
        )

    def _pick_word(self):
        i = int(xos.random.randint(0, self.n - 1))
        self.current = xos.csv.row(self.table, i)

    def _study_viewport_ui_coef(self):
        """Single UI zoom coefficient for Study: mirrors F3/Application.xos_scale (percent / 100)."""
        return float(getattr(self, "xos_scale", 1.0))

    def _hero_norm_x(self, word, frame_w_px, band_x1, band_x2):
        """Trimmed glyph band for the vocab, horizontally centered inside the chat column `[band_x1, band_x2]`."""
        sc = self._study_viewport_ui_coef()
        fs = float(self.hero.font_size) * sc
        n = max(1, len(word))
        tw = fs * max(1.25, float(n) * 0.9)
        bw = max(1e-6, (band_x2 - band_x1) * frame_w_px)
        tw = min(tw, bw * 0.98)
        cx = (band_x1 + band_x2) * 0.5
        hw_norm = tw / (2.0 * frame_w_px)
        eps = 1e-4
        x1 = max(band_x1 + eps, cx - hw_norm)
        x2 = min(band_x2 - eps, cx + hw_norm)
        return x1, x2

    def _composer_layout(self):
        w, h = self.get_width(), self.get_height()
        coef = self._study_viewport_ui_coef()
        ktop = float(getattr(self, "keyboard_top_y", 1.0))
        # Float geometry so margins scale smoothly with coef (fonts already scale via the same coef).
        margin_px = max(10.0, 12.0 * coef)
        gap_px = max(4.0, 6.0 * coef)
        comp_h = max(44.0, 50.0 * coef)
        bottom_px = min(float(h), ktop * float(h) - gap_px)
        top_px = bottom_px - comp_h
        min_top = 0.22 * float(h)
        top_px = max(top_px, min_top)
        ml = margin_px
        mr = float(w) - margin_px
        x1n = ml / float(w)
        x2n = mr / float(w)
        y1n = top_px / float(h)
        y2n = bottom_px / float(h)
        pad_px = max(8.0, 10.0 * coef)
        pad_x = pad_px / float(w)
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
        head = "&a&lCorrect!&r&r\n" if ok else "&c&lIncorrect&r&r\n"
        line1 = (
            f"{head}"
            f"&e「{vk}」&rは&d&l{vword}&r。\n"
            f"&9({mean})&r"
        )
        parts = [
            line1,
            "",
            f"&b&l{ja}&r",
            f"&b{kana}&r",
            f"&9&l{en}&r",
            "",
            "&e&lDrag to select&r · &l&dEnter continues&r\n"
            "&7Double-tap the bar ⇄ on-screen keyboard&r",
        ]
        return "\n".join(parts)

    def _clear_feedback_selection(self):
        self.fb_dragging = False
        self.fb_anchor = None
        self.fb_sel = None

    def on_viewport_double_tap(self, x, y):
        lay = self._composer_layout()
        ox1, oy1, ox2, oy2 = lay["outer"]
        fw, fh = self.get_width(), self.get_height()
        nx = float(x) / float(fw)
        ny = float(y) / float(fh)
        if ox1 <= nx <= ox2 and oy1 <= ny <= oy2:
            xos.keyboard.toggle_onscreen()

    def on_mouse_down(self, x, y):
        fw, fh = self.get_width(), self.get_height()
        lay = self._composer_layout()
        ox1, oy1, ox2, oy2 = lay["outer"]
        nx = float(x) / float(fw)
        ny = float(y) / float(fh)
        in_comp = ox1 <= nx <= ox2 and oy1 <= ny <= oy2
        if self.state == "feedback":
            if in_comp:
                return
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
        # Same knob as rasterized text (`_viewport_scaled_font` uses Application.xos_scale).
        self.viewport_ui_coefficient = self._study_viewport_ui_coef()
        lay = self._composer_layout()
        ox1, oy1, ox2, oy2 = lay["outer"]
        ix1, iy1, ix2, iy2 = lay["inner"]

        # Hero panel + caption/thread/feedback share the composer’s horizontal band exactly (`ox1`…`ox2`).
        cy1 = 0.032
        cy2 = 0.158
        xos.rasterizer.rects_filled(
            self.frame,
            int(ox1 * w),
            int(cy1 * h),
            int(ox2 * w),
            int(cy2 * h),
            CLR_CARD,
        )
        # rectangles() expects flat [x1,y1,x2,y2] normalized, not nested lists.
        xos.rasterizer.rectangles(
            self.frame,
            [ox1, cy1, ox2, cy2],
            CLR_CARD_EDGE[:3],
            1.75,
        )

        self.caption.x1 = ox1
        self.caption.x2 = ox2
        self.thread_status.x1 = ox1
        self.thread_status.x2 = ox2
        self.feedback_ui.x1 = ox1
        self.feedback_ui.x2 = ox2

        word = self.current.get(VOCAB_COL, "") if self.current else ""
        self.hero.text = word
        hx1, hx2 = self._hero_norm_x(word, w, ox1, ox2)
        self.hero.x1 = hx1
        self.hero.x2 = hx2
        self.hero.render(self.frame)

        self.caption.render(self.frame)

        ft_y2 = oy1 - 0.015
        self.feedback_ui.y2 = max(0.24, min(0.92, ft_y2))

        if self.state == "prompt":
            self.feedback_ui.text = ""
            self.thread_status.color = CLR_THREAD[:3]
            self.chat_value.mutable = True
            self.chat_value.show_cursor = True
            self.thread_status.text = (
                "Double-tap the bar ⇄ keyboard"
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
                1.25,
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
            self.composer_hint.text = ""

        else:
            self.thread_status.text = "Review · your answer is frozen below"
            self.thread_status.color = CLR_THREAD[:3]
            self.thread_status.render(self.frame)

            self.chat_value.mutable = False
            self.chat_value.show_cursor = False

            span = iy2 - iy1
            mid_y = iy1 + 0.58 * span
            self.chat_value.x1 = ix1
            self.chat_value.y1 = iy1
            self.chat_value.x2 = ix2
            self.chat_value.y2 = mid_y
            self.composer_hint.x1 = ix1
            self.composer_hint.y1 = mid_y
            self.composer_hint.x2 = ix2
            self.composer_hint.y2 = iy2

            self.feedback_ui.text = self.feedback_text
            lo, hi = (-1, -1)
            if self.fb_sel is not None:
                lo, hi = self.fb_sel
            self.feedback_ui.render(
                self.frame, selection_start=lo, selection_end=hi
            )

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
                CLR_HINT_BAND[:3],
                1.25,
            )
            gb = self.guess_buf.strip()
            self.chat_value.text = gb if gb else "—"
            self.chat_value.render(self.frame)

            kb_on = bool(getattr(self, "onscreen_keyboard_visible", False))
            self.composer_hint.text = (
                "⌨ Tap Enter on keyboard · next ·  double-tap bar ⇄ keyboard"
                if kb_on
                else "Enter on keyboard advances · double-tap this bar ⇄ keyboard"
            )
            self.composer_hint.render(self.frame)

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
