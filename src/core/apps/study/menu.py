MENU_LEFT_EDGE_EXTENSION = 0.3
MENU_BUTTON_WIDTH = 0.1


class Menu:

    def __init__(self):
        self.is_open = False

        self.buttons = self.setup_buttons()
        self.menu_display = self.setup_display()


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

    def tick(self, app):
        self.menu_button_background.set_verts(*self.menu_button.verts)

    def render(self, app):
        self.buttons.render(app)
        self.menu_display.render(app)

    # def _show_menu(self):
    #     self.menu_visible = not self.menu_visible

    #     if self.menu_visible:
    #         self.menu_background.x1 = MENU_LEFT_EDGE_EXTENSION
    #         self.menu_button.x1 = MENU_LEFT_EDGE_EXTENSION
    #         self.menu_button.x2 = MENU_LEFT_EDGE_EXTENSION
    #     else:
    #         self.menu_background.x1 = 0.0


    def toggle_menu(self):
        # TODO: make it so that the edges stay aligned and such
        # set_verts will be useful, and the calculations of those
        # normalized edge coordinates will also be helpful

        raise NotImplementedError("toggle_menu is not implemented")

    def on_events(self, app):
        self.buttons.on_events(app)
        self.menu_display.on_events(app)