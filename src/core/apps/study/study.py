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


# Minimal UI: monochrome surfaces and gray type on black (Minecraft-rich feedback uses matching &codes).
CLR_BG = (8, 8, 10, 255)
CLR_CARD = (22, 22, 26, 255)
CLR_CARD_EDGE = (68, 68, 74, 255)
CLR_COMPOSER = (18, 18, 22, 255)
CLR_COMPOSER_EDGE = (90, 90, 96, 255)
CLR_MUTED = (130, 130, 138, 255)
CLR_THREAD = (205, 205, 212, 255)
CLR_CAPTION = (178, 178, 184, 255)
# Hero cue — same hue as Minecraft ``&a`` used for vocab/readings in review rich text.
CLR_KANJI = (85, 255, 85, 255)
CLR_HINT_BAND = (52, 52, 56, 255)

# Scale for the review block (feedback rich text + thread line + composer summary in feedback mode).
REVIEW_TEXT_SCALE = 1.3 * 0.8

# Global Study UI vs earlier revision (~+50% type, chrome, composer, vertical bands).
STUDY_UI_SCALE = 1.5

# Viewport text sizes: bases below are multiplied inside Text/RichText.render by `xos.Application.xos_scale`
# (same value as `_study_viewport_ui_coef()` drives for composer geometry below).
FS = 1.2 * STUDY_UI_SCALE
# Kanji cue line — extra clarity for handwriting along in a notebook (~20 % larger than previous study hero).
HERO_FONT_BASE = 48.0 * FS * 1.2


def _sentence_with_lime_vocab(s, vword):
    """Minecraft markup: lime (``&a``) vocab hits; intervening prose bold white."""
    if not vword:
        return f"&f&l{s}&r"
    s = str(s)
    parts = s.split(str(vword))
    if len(parts) == 1:
        return f"&f&l{s}&r"
    chunks = []
    for i, p in enumerate(parts):
        if p:
            chunks.append(f"&f&l{p}&r")
        if i < len(parts) - 1:
            chunks.append(f"&a&l{vword}&r")
    return "".join(chunks)


def _kana_line_with_lime_reading(kana_line, vk):
    """Sentence kana row: occurrences of vocab reading ``vk`` in lime, rest muted ``&7``."""
    ks = str(kana_line)
    vk = str(vk) if vk is not None else ""
    if not vk or vk not in ks:
        return f"&7{ks}&r"
    parts = ks.split(vk)
    chunks = []
    for i, p in enumerate(parts):
        if p:
            chunks.append(f"&7{p}&r")
        if i < len(parts) - 1:
            chunks.append(f"&a&l{vk}&r")
    return "".join(chunks)


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
            font_size=19.0 * FS * REVIEW_TEXT_SCALE,
        )
        self.feedback_ui = xos.ui.rich_text(
            "",
            0.04,
            0.230,
            0.96,
            0.88,
            color=(220, 220, 226),
            font_size=21.0 * FS * REVIEW_TEXT_SCALE,
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
            font_size=15.0 * FS * REVIEW_TEXT_SCALE,
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

    def _safe_bounds(self):
        """Framebuffer-normalized safe area (matches ``FrameState.safe_region_boundaries``)."""
        d = getattr(self.frame, "_data", {})
        try:
            return (
                float(d.get("safe_x1", 0.0)),
                float(d.get("safe_y1", 0.0)),
                float(d.get("safe_x2", 1.0)),
                float(d.get("safe_y2", 1.0)),
            )
        except (TypeError, ValueError):
            return (0.0, 0.0, 1.0, 1.0)

    def _map_layout_y_norm(self, y):
        """Map a legacy layout Y in 0..1 (full framebuffer) into the vertical safe strip."""
        _sx1, sy1, _sx2, sy2 = self._safe_bounds()
        span = max(0.0, sy2 - sy1)
        return sy1 + float(y) * span

    def _study_vertical_bands_mapped(self):
        """Legacy-space Y bands scaled vertically so larger fonts still fit."""
        k = STUDY_UI_SCALE
        gap = 0.006 * k
        hm = (0.048 + 0.126) / 2.0
        hh = (0.126 - 0.048) * k
        hy1l = hm - hh / 2.0
        hy2l = hm + hh / 2.0
        cy1l = hy1l - (0.048 - 0.032) * k
        cy2l = hy2l + (0.158 - 0.126) * k
        cm = (0.129 + 0.168) / 2.0
        ch = (0.168 - 0.129) * k
        cap_y1l = max(cm - ch / 2.0, cy2l + gap)
        cap_y2l = cap_y1l + ch
        tm = (0.176 + 0.222) / 2.0
        th = (0.222 - 0.176) * k
        thr_y1l = max(tm - th / 2.0, cap_y2l + gap)
        thr_y2l = thr_y1l + th
        fb_y1l = max(0.230, thr_y2l + gap)
        map_y = self._map_layout_y_norm
        return {
            "cy1": map_y(cy1l),
            "cy2": map_y(cy2l),
            "hero_y1": map_y(hy1l),
            "hero_y2": map_y(hy2l),
            "caption_y1": map_y(cap_y1l),
            "caption_y2": map_y(cap_y2l),
            "thread_y1": map_y(thr_y1l),
            "thread_y2": map_y(thr_y2l),
            "feedback_y1": map_y(fb_y1l),
            "feedback_y2_min": map_y(0.24 * k),
        }

    def _hero_norm_x(self, word, frame_w_px, band_x1, band_x2):
        """Kanji cue: width from a generous CJK estimate, centered in the composer column."""
        eps = 4e-4
        sc = self._study_viewport_ui_coef()
        fs = float(self.hero.font_size) * sc
        bw_px = max(1.0, (band_x2 - band_x1) * frame_w_px)
        n = max(1, len(word))
        # Wide enough per glyph that short vocab does not wrap; cap so long strings still use most of band.
        tw_px = fs * max(2.25, float(n) * 1.14)
        tw_px = min(tw_px, bw_px * 0.995)
        cx = (band_x1 + band_x2) * 0.5
        hw_norm = tw_px / (2.0 * frame_w_px)
        x1 = max(band_x1 + eps, cx - hw_norm)
        x2 = min(band_x2 - eps, cx + hw_norm)
        return x1, x2

    def _composer_layout(self):
        w, h = self.get_width(), self.get_height()
        sx1, sy1, sx2, sy2 = self._safe_bounds()
        coef = self._study_viewport_ui_coef()
        ktop = float(getattr(self, "keyboard_top_y", 1.0))
        # Float geometry so margins scale smoothly with coef (fonts already scale via the same coef).
        safe_left_px = sx1 * float(w)
        safe_right_px = sx2 * float(w)
        safe_top_px = sy1 * float(h)
        safe_bottom_px = sy2 * float(h)
        s = STUDY_UI_SCALE
        margin_px = max(10.0, 12.0 * coef) * s
        gap_px = max(4.0, 6.0 * coef) * s
        # ~30% taller composer bar (hit target + romaji line).
        comp_h = 1.3 * max(44.0, 50.0 * coef) * s
        bottom_px = min(safe_bottom_px, ktop * float(h) - gap_px)
        top_px = bottom_px - comp_h
        sh = max(1.0, safe_bottom_px - safe_top_px)
        min_top = safe_top_px + 0.22 * sh
        top_px = max(top_px, min_top)
        ml = safe_left_px + margin_px
        mr = safe_right_px - margin_px
        x1n = ml / float(w)
        x2n = mr / float(w)
        y1n = top_px / float(h)
        y2n = bottom_px / float(h)
        pad_px = max(8.0, 10.0 * coef) * s
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
        # &a = Minecraft bright lime/green over black; lime matches ``CLR_KANJI`` accent.
        head = "&f&lCorrect!&r&r\n" if ok else "&8&lIncorrect&r&r\n"
        line1 = (
            f"{head}"
            f"&f「&a&l{vk}&r&f」は&a&l{vword}&r。\n"
            f"&8({mean})&r"
        )
        parts = [
            line1,
            "",
            _sentence_with_lime_vocab(ja, vword),
            _kana_line_with_lime_reading(kana, vk),
            f"&f&l{en}&r",
            "",
            "&7&lDrag to select&r · &r&8Enter continues&r\n"
            "&8Double-tap the bar ⇄ on-screen keyboard&r",
        ]
        return "\n".join(parts)

    def _clear_feedback_selection(self):
        self.fb_dragging = False
        self.fb_anchor = None
        self.fb_sel = None

    def _feedback_selection_plain_slice(self):
        """Selected substring in markdown-free plain indices (matches ``RichText.pick`` / highlight)."""
        if self.state != "feedback" or self.fb_sel is None:
            return ""
        lo, hi = self.fb_sel
        plain = self.feedback_ui.plain()
        n = len(plain)
        lo = max(0, min(int(lo), n))
        hi = max(0, min(int(hi), n))
        if hi <= lo:
            return ""
        return plain[lo:hi]

    def on_key_shortcut(self, action):
        """Desktop Cmd/Ctrl and OSK action row — share implementation for copy/paste."""
        if action == "copy":
            chunk = self._feedback_selection_plain_slice()
            if not chunk and self.state == "prompt" and self.guess_buf:
                chunk = self.guess_buf
            if chunk:
                xos.clipboard.set(chunk)
            return

        if action == "cut":
            if self.state == "feedback":
                chunk = self._feedback_selection_plain_slice()
                if chunk:
                    xos.clipboard.set(chunk)
                return
            if (
                self.state == "prompt"
                and getattr(self.chat_value, "mutable", False)
                and self.guess_buf
            ):
                xos.clipboard.set(self.guess_buf)
                self.guess_buf = ""
            return

        if action == "paste":
            if self.state != "prompt" or not getattr(self.chat_value, "mutable", False):
                return
            text = (
                xos.clipboard.get()
                .replace("\r\n", "\n")
                .replace("\r", "\n")
                .split("\n", 1)[0]
            )
            if not text:
                return
            for ch in text:
                if ord(ch) < 32 and ch != "\t":
                    continue
                if len(self.guess_buf) >= 160:
                    break
                self.guess_buf += ch
            return

        if action == "select_all":
            if self.state == "feedback":
                plain = self.feedback_ui.plain()
                self.fb_sel = (0, len(plain))
            return

        # undo / redo — study has no stacks; subclasses can extend.

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
        _sx1, _sy1, _sx2, sy2 = self._safe_bounds()
        vb = self._study_vertical_bands_mapped()
        strk_outer = 1.75 * STUDY_UI_SCALE
        strk_comp = 1.25 * STUDY_UI_SCALE

        # Hero panel + caption/thread/feedback share the composer’s horizontal band exactly (`ox1`…`ox2`).
        cy1, cy2 = vb["cy1"], vb["cy2"]
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
            strk_outer,
        )

        self.caption.x1 = ox1
        self.caption.x2 = ox2
        self.caption.y1 = vb["caption_y1"]
        self.caption.y2 = vb["caption_y2"]
        self.thread_status.x1 = ox1
        self.thread_status.x2 = ox2
        self.thread_status.y1 = vb["thread_y1"]
        self.thread_status.y2 = vb["thread_y2"]
        self.feedback_ui.x1 = ox1
        self.feedback_ui.x2 = ox2
        self.feedback_ui.y1 = vb["feedback_y1"]

        word = self.current.get(VOCAB_COL, "") if self.current else ""
        self.hero.text = word
        hx1, hx2 = self._hero_norm_x(word, w, ox1, ox2)
        self.hero.x1 = hx1
        self.hero.x2 = hx2
        self.hero.y1 = vb["hero_y1"]
        self.hero.y2 = vb["hero_y2"]
        self.hero.render(self.frame)

        self.caption.render(self.frame)

        ft_y2 = oy1 - 0.015 * STUDY_UI_SCALE
        y_lo = vb["feedback_y2_min"]
        y_hi = min(self._map_layout_y_norm(0.92), sy2 - 2e-3)
        self.feedback_ui.y2 = max(y_lo, min(y_hi, ft_y2))

        prompt_fs = 22.0 * FS
        review_fs = prompt_fs * REVIEW_TEXT_SCALE

        if self.state == "prompt":
            self.feedback_ui.text = ""
            self.thread_status.color = CLR_THREAD[:3]
            self.chat_value.font_size = prompt_fs
            self.chat_placeholder.font_size = prompt_fs
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
                strk_comp,
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
            self.chat_value.font_size = review_fs
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
                strk_comp,
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
