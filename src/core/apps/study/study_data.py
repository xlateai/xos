"""
Load Japanese Vocab 6k from Hugging Face (cached under ``xos.path.dotxos``), row sampling, and answer check.
No score persistence — download + in-memory play only.
"""
import xos

# API shape requested by the app author
HF_VOCAB_CSV = "https://huggingface.co/datasets/nollied/Japanese-Vocab-6k/resolve/main/japanese_vocabs_6000.csv?download=true"

study_data_folder = xos.path.dotxos / "data" / "study"
csv_path = study_data_folder / "japanese_vocabs_6000.csv"

# JLPT column and level mapping match ``xlate/dashboards/utils.py`` / ``LEVEL_COL``.
# CSV cells look like ``JLPT4``; ``int(first_digit) + 1`` yields dashboard level. Level ``5``
# is the N5-tier slice (``shared_kanji_graph.py`` uses ``LEVEL_COL == 5`` as N5-only).
JLPT_COL_CANDIDATES = ("jlpt", "jlpt ")
DEFAULT_JLPT_DASHBOARD_LEVEL = 5  # N5

# Avoid ``unicodedata`` / stdlib ``re``: compare after lowercasing and dropping common spaces only.
_WS_SKIP = frozenset(" \t\n\r\f\v\u3000")

# --- Transcription helpers (aligned with ``xlate/dashboards/utils.py`` + ``guessing_game.py``) ---
# CSV ``Vocab-kana`` is often katakana (e.g. loanwords); users type hiragana. We must normalize
# like ``katakana_to_hiragana(Vocab-kana)`` before comparing. Romaji typing uses the same strict
# ``nn`` → ん rule and sokuon doubling as the terminal game.

hiragana_map = {
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


def _katakana_to_hiragana(s: str) -> str:
    out = []
    for char in str(s):
        o = ord(char)
        if 0x30A0 <= o <= 0x30FF:
            if char == "ー":
                out.append(char)
            else:
                out.append(chr(o - 0x60))
        else:
            out.append(char)
    return "".join(out)


def _romaji_to_hiragana(romaji: str) -> str:
    romaji = romaji.lower()
    sorted_keys = sorted(hiragana_map.keys(), key=lambda x: len(x), reverse=True)
    h = ""
    i = 0
    n = len(romaji)
    while i < n:
        if romaji[i] == "n":
            if i + 1 < n and romaji[i + 1] == "n":
                h += "ん"
                i += 2
                continue
            if i + 1 < n and romaji[i + 1] in ["a", "i", "u", "e", "o", "y"]:
                pass
            else:
                h += "n"
                i += 1
                continue
        if i < n - 1 and romaji[i] == romaji[i + 1] and romaji[i] not in ["a", "i", "u", "e", "o", "n"]:
            h += "っ"
            i += 1
            continue
        matched = False
        for key in sorted_keys:
            if romaji[i:].startswith(key):
                if key == "ji" and romaji[i - 1 : i] in ["d"]:
                    h += hiragana_map["ji_d"]
                else:
                    h += hiragana_map[key]
                i += len(key)
                matched = True
                break
        if not matched:
            h += romaji[i]
            i += 1
    return h


def _unicode_has_kana(s: str) -> bool:
    for c in s:
        o = ord(c)
        if 0x3040 <= o <= 0x309F or 0x30A0 <= o <= 0x30FF:
            return True
    return False


def _normalize_romaji_guess(s: str) -> str:
    t = str(s).strip().lower()
    return xos.regex.sub(r"\s+", "", t)


def _strip_html(s: str) -> str:
    return xos.regex.sub(r"<[^>]+>", "", str(s))


def _norm_typing(s: str) -> str:
    t = str(s).strip().lower()
    return "".join(c for c in t if c not in _WS_SKIP)


def _canonical_kana_key(s: str) -> str:
    """Single string form for equality checks (loanword katakana vs IME hiragana)."""
    return _norm_typing(_katakana_to_hiragana(str(s)))


def _col(row: dict, *names: str) -> str:
    for n in names:
        if n in row and row[n] is not None:
            return str(row[n])
    return ""


def _jlpt_dashboard_level(cell: str):
    """First digit in the cell ``+ 1``, same as Polars expr in ``get_vocab_data``."""
    for ch in str(cell).strip():
        if ch.isdigit():
            return int(ch) + 1
    return None


def _filtered_vocab_rows(csv_table, jlpt_dashboard_level):
    rows = []
    n = len(csv_table)
    for i in range(n):
        row = dict(csv_table[i])
        lv = _jlpt_dashboard_level(_col(row, *JLPT_COL_CANDIDATES))
        if lv == jlpt_dashboard_level:
            rows.append(row)
    return rows


class StudyData:
    """Loads filtered vocab rows and checks guesses like ``guessing_game`` (katakana key + IME / romaji)."""

    def __init__(self, jlpt_dashboard_level=None) -> None:
        study_data_folder.makedirs(exists_ok=True)
        dest = str(csv_path)
        if not csv_path.exists():
            xos.data.download(HF_VOCAB_CSV, dest)
        full = xos.csv.load(dest)
        target = (
            jlpt_dashboard_level
            if jlpt_dashboard_level is not None
            else DEFAULT_JLPT_DASHBOARD_LEVEL
        )
        self._rows = _filtered_vocab_rows(full, int(target))
        self._n = len(self._rows)
        if self._n < 1:
            raise RuntimeError(
                "study: no vocabulary rows for JLPT filter (dashboard level {}). "
                "Try a different jlpt_dashboard_level or check CSV column 'jlpt'.".format(target)
            )
        self.current = None

    def next_example(self) -> dict:
        """Pick a random row and store it as ``self.current``."""
        i = xos.random.randint(0, self._n - 1)
        self.current = self._rows[i]
        return self.current

    def check_answer(self, guess: str) -> tuple:
        """
        Compare ``guess`` to ``Vocab-kana``: answer key as hiragana (``katakana_to_hiragana`` CSV),
        guess as IME kana **or** ASCII romaji (``nn`` → ん, doubled consonants → っ), matching
        ``xlate/dashboards/guessing_game.py``.
        Returns ``(ok, short_message)``.
        """
        row = self.current
        if row is None:
            return False, "No word loaded."
        target = _col(row, "Vocab-kana")
        if not target:
            return False, "Missing answer in data row."

        answer_key = _canonical_kana_key(target)
        g = str(guess).strip()
        hiragana_candidates = []
        if _unicode_has_kana(g):
            hiragana_candidates.append(_canonical_kana_key(g))
        else:
            rj = _normalize_romaji_guess(g)
            if rj != "":
                hiragana_candidates.append(_canonical_kana_key(_romaji_to_hiragana(rj)))

        ok = any(h == answer_key for h in hiragana_candidates)

        kanji = _col(row, "Vocab-expression")
        meaning = _col(row, "Vocab-meaning")
        if ok:
            return True, f"Correct! 「{target}」 — {kanji} ({meaning})"
        if g == "":
            return False, f"Reading: 「{target}」 — {kanji} ({meaning})"
        return False, f"Not quite. Reading: 「{target}」 — {kanji} ({meaning})"

    def prompt_markup(self) -> str:
        return (
            "Type the reading in hiragana (or match the kana).\n"
            "[Double tap anywhere to open the on-screen keyboard.](color=GRAY size=32)\n"
            "[Press Enter / return when done.](color=GRAY size=28)"
        )

    def breakdown_markup(self, ok: bool, empty_guess: bool = False) -> str:
        """Colored sentence + meaning like the terminal guessing game, without HTML tags."""
        row = self.current or {}
        if ok:
            verdict = "[Correct!](color=LIME size=40)"
        elif empty_guess:
            verdict = "[Answer](color=CYAN size=40)"
        else:
            verdict = "[Try again](color=ORANGE size=40)"
        w = _col(row, "Vocab-expression")
        k = _col(row, "Vocab-kana")
        m = _col(row, "Vocab-meaning")
        jp = _strip_html(_col(row, "Sentence-expression"))
        jpk = _strip_html(_col(row, "Sentence-kana"))
        en = _strip_html(_col(row, "Sentence-meaning"))
        lines = [
            verdict,
            f"[{w}](size=56)  [{k}](color=CYAN size=44)",
            f"[{m}](color=GRAY size=36)",
            f"[{jp}](color=WHITE size=34)",
            f"[{jpk}](color=CYAN size=32)",
            f"[{en}](color=LIGHT_BLUE size=32)",
            "[Press Enter (empty guess) for the next word.](color=GRAY size=28)",
        ]
        return "\n".join(lines)
