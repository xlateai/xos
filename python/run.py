import xospy

# run a rust compiled game with the game name here
xospy.run_game("ball", web=False, react_native=False)

# class PyApp(xospy.ApplicationBase):
#     def setup(self, width: int, height: int):
#         pass

#     def tick(self, width: int, height: int):
#         return [0, 100, 100, 255] * (width * height)  # RGBA black pixels

# xospy.run_py_game(PyApp(), web=False, react_native=False)