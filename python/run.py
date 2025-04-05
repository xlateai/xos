import xospy

# run a rust compiled game with the game name here
# xospy.run_game("tracers", web=False, react_native=False)

class PyApp(xospy.ApplicationBase):
    def setup(self, width: int, height: int):
        self.counter = 0

    def tick(self, width: int, height: int):
        self.counter += 1
        print("tick", width, height, self.counter)
        return [0, 100, 100, 255] * (width * height)

xospy.run_py_game(PyApp(), web=False, react_native=False)