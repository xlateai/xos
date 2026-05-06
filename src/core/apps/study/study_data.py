"""
Load Japanese Vocab 6k from Hugging Face (cached under ``xos.path.dotxos``), row sampling, and answer check.
No score persistence — download + in-memory play only.
"""
import xos

# API shape requested by the app author
HF_VOCAB_CSV = "https://huggingface.co/datasets/nollied/Japanese-Vocab-6k/resolve/main/japanese_vocabs_6000.csv?download=true"

study_data_folder = xos.path.dotxos / "data" / "study"
csv_path = study_data_folder / "japanese_vocabs_6000.csv"

# Avoid ``unicodedata`` / stdlib ``re``: compare after lowercasing and dropping common spaces only.
_WS_SKIP = frozenset(" \t\n\r\f\v\u3000")


def _strip_html(s: str) -> str:
    return xos.regex.sub(r"<[^>]+>", "", str(s))


def _norm_typing(s: str) -> str:
    t = str(s).strip().lower()
    return "".join(c for c in t if c not in _WS_SKIP)


def _col(row: dict, *names: str) -> str:
    for n in names:
        if n in row and row[n] is not None:
            return str(row[n])
    return ""


class StudyData:
    """Holds the loaded CSV, current row, and a light kana equality check (no NFKC dependency)."""

    def __init__(self) -> None:
        study_data_folder.makedirs(exists_ok=True)
        dest = str(csv_path)
        if not csv_path.exists():
            xos.data.download(HF_VOCAB_CSV, dest)
        self._csv = xos.csv.load(dest)
        self._n = len(self._csv)
        if self._n < 1:
            raise RuntimeError("study: vocabulary CSV is empty")
        self.current = None

    def next_example(self) -> dict:
        """Pick a random row and store it as ``self.current``."""
        i = xos.random.randint(0, self._n - 1)
        self.current = self._csv[i]
        return self.current

    def check_answer(self, guess: str) -> tuple:
        """
        Compare ``guess`` to the reading (``Vocab-kana``) after normalization.
        Returns ``(ok, short_message)``.
        """
        row = self.current
        if row is None:
            return False, "No word loaded."
        target = _col(row, "Vocab-kana")
        if not target:
            return False, "Missing answer in data row."
        ok = _norm_typing(guess) == _norm_typing(target)
        kanji = _col(row, "Vocab-expression")
        meaning = _col(row, "Vocab-meaning")
        if ok:
            return True, f"Correct! 「{target}」 — {kanji} ({meaning})"
        return False, f"Not quite. Reading: 「{target}」 — {kanji} ({meaning})"

    def prompt_markup(self) -> str:
        return (
            "Type the reading in hiragana (or match the kana).\n"
            "[Double tap anywhere to open the on-screen keyboard.](color=GRAY size=32)\n"
            "[Press Enter / return when done.](color=GRAY size=28)"
        )

    def breakdown_markup(self, ok: bool) -> str:
        """Colored sentence + meaning like the terminal guessing game, without HTML tags."""
        row = self.current or {}
        verdict = "[Correct!](color=LIME size=40)" if ok else "[Try again](color=ORANGE size=40)"
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
