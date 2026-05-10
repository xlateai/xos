




def get_buttons():
    menu_button = xos.ui.button(
        0.0,
        0.0,
        0.1,
        0.1,
        on_press=self._show_menu,
    )

    menu_button_background = xos.ui.rect(
        0.0,
        0.0,
        0.0,
        1.0,
        color=xos.color.BLACK,
        # alpha=0.5,
    )

    return xos.ui.group(
        menu_button,
        menu_button_background,
    )

def get_menu():
    background = xos.ui.rect(
        0.0,
        0.0,
        0.0,
        1.0,
        color=xos.color.BLACK,
        # alpha=0.5,
    )

    return xos.ui.group(background)