import xos

MENU_LEFT_EDGE_EXTENSION = 0.3
MENU_BUTTON_WIDTH = 0.05

MENU_BUTTON_CLOSED_VERTS = (
    0.0,
    0.0,
    MENU_BUTTON_WIDTH,
    MENU_BUTTON_WIDTH,
)

MENU_BUTTON_OPEN_VERTS = (
    MENU_LEFT_EDGE_EXTENSION + 0.0,
    0.0,
    MENU_LEFT_EDGE_EXTENSION + MENU_BUTTON_WIDTH,
    MENU_BUTTON_WIDTH,
)

MENU_BUTTON_COORDINATE_SYSTEM = (
    xos.coordinates.VIEWPORT_MAX_DIMENSION,
    xos.coordinates.VIEWPORT_MAX_DIMENSION,
    xos.coordinates.VIEWPORT_MAX_DIMENSION,
    xos.coordinates.VIEWPORT_MAX_DIMENSION,
)



class Menu:

    def __init__(self):
        self.is_open = False

        self.buttons = self.setup_buttons()
        self.menu_display = self.setup_display()


    def setup_buttons(self):
        self.menu_button = xos.ui.button(
            *MENU_BUTTON_CLOSED_VERTS,
            coordinate_system=MENU_BUTTON_COORDINATE_SYSTEM,
            on_press=self.toggle_menu,
        )

        self.menu_button_background = xos.ui.rect(
            *MENU_BUTTON_CLOSED_VERTS,
            color=xos.color.LIME,
            coordinate_system=MENU_BUTTON_COORDINATE_SYSTEM,
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
            coordinate_system=MENU_BUTTON_COORDINATE_SYSTEM,
            # alpha=0.5,
        )

        return xos.ui.group(
            self.menu_background,
        )

    def update_button_verts(self):
        if self.is_open:
            self.menu_background.x1 = MENU_LEFT_EDGE_EXTENSION
            self.menu_button.verts = MENU_BUTTON_OPEN_VERTS
        else:
            self.menu_background.x1 = 0.0
            self.menu_button.verts = MENU_BUTTON_CLOSED_VERTS

        self.menu_button_background.verts = self.menu_button.verts

    def tick(self, app):
        self.update_button_verts()

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
        self.is_open = not self.is_open

    def on_events(self, app):
        self.buttons.on_events(app)
        self.menu_display.on_events(app)