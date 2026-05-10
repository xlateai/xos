import xos



class CoordinateSystem:
    # basically, the idea is that we can have
    # a arbitrary dimensionality coordinate system
    # vector that represents convertible coordinates
    # through multiple systems.
    
    # the idea is that you define multiple types of
    # coordinate systems and then any time you access
    # or update those coordinates, it reverse transforms
    # into the base coordinate system, and any time
    # it is accessed from other coordinate systems, it
    # is transformed into the other coordinate system.

    def __init__(self):
        pass