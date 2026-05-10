MENU_LEFT_EDGE_EXTENSION = 0.3
MENU_BUTTON_WIDTH = 0.1


class Menu:

    def __init__(self):
        self.is_open = False

        self.buttons = self.setup_buttons()
        self.menu_display = self.setup_display()


    def toggle_menu(self):
        raise NotImplementedError("toggle_menu is not implemented")


    def setup_buttons(self):
        self.menu_button = xos.ui.button(
            0.0,
            0.0,
            0.1,
            0.1,
            on_press=self.toggle_menu,
        )

        self.menu_button_background = xos.ui.rect(
            color=xos.color.BLACK,
            # alpha=0.5,
        )

        return xos.ui.group(
            self.menu_button,
            self.menu_button_background,
        )

    def setup_display(self):
        self.menu_background = xos.ui.rect(
            0.0,
            0.0,
            0.0,
            1.0,
            color=xos.color.BLACK,
            # alpha=0.5,
        )

        return xos.ui.group(
            self.menu_background,
        )

    def tick(self):
        self.menu_button_background.set_verts(*self.menu_button.verts)

    def render(self):
        pass

    # def on_events(self, app):
        # self.buttons.on_events(app)