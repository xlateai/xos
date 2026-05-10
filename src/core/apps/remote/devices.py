"""UI helpers for the remote app (copy the style of ``study/menu.py``: small factories)."""

import xos


def status_line(initial: str):
    return xos.ui.text(
        initial,
        x1=0.04,
        y1=0.02,
        x2=0.72,
        y2=0.10,
        editable=False,
        selectable=False,
        scrollable=False,
        show_cursor=False,
        size=18.0,
        color=xos.color.GRAY,
        alignment=(0.0, 0.5),
    )


def reconnect_button(on_press):
    return xos.ui.button(
        0.72,
        0.02,
        0.98,
        0.10,
        on_press=on_press,
    )


def back_button(on_press):
    return xos.ui.button(0.02, 0.02, 0.30, 0.10, on_press=on_press)
