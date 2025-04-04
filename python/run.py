import xos


# run a rust compiled game with the game name here
# xos.run_game("ball", web=False, react_native=False)

class PyApp(xos.ApplicationBase):
    def setup(self, width: int, height: int):
        pass

    def tick(self, width: int, height: int):
        return [0, 100, 100, 255] * (width * height)  # RGBA black pixels

xos.run_game(PyApp(), web=False, react_native=False)