# Loaded into `xos.system` — native hooks `_system_monitor_*` must be injected from Rust.


class Monitor:
    """One physical/virtual display (`xos.system.monitors[index]`).

    Sizes are pixels. ``width``/``height`` are the native-resolution capture size;
    ``stream_width``/``stream_height`` match frames returned by ``get_frame()`` (downscaled LAN cap).
    """

    def __init__(self, index):
        self._index = index
        self._meta = None

    def _meta_dict(self):
        if self._meta is None:
            self._meta = _system_monitor_meta(self._index)
        return self._meta

    @property
    def width(self):
        return self._meta_dict()["native_width"]

    @property
    def height(self):
        return self._meta_dict()["native_height"]

    @property
    def stream_width(self):
        return self._meta_dict()["stream_width"]

    @property
    def stream_height(self):
        return self._meta_dict()["stream_height"]

    @property
    def x(self):
        return self._meta_dict()["origin_x"]

    @property
    def y(self):
        return self._meta_dict()["origin_y"]

    @property
    def refresh_rate(self):
        """Refresh rate in Hz, or ``0`` if unknown."""
        return self._meta_dict()["refresh_rate_hz"]

    @property
    def is_primary(self):
        return self._meta_dict()["is_primary"]

    @property
    def name(self):
        return self._meta_dict()["name"]

    @property
    def native_id(self):
        """Platform display identifier string (blank if unsupported)."""
        return self._meta_dict()["native_id"]

    def get_frame(self):
        """Captured RGBA **Frame**, same downscale policy as LAN ``remote_frame`` streams.

        Implemented on desktop macOS / Windows / Linux stubs (empty list) today; absent on WASM / iOS.
        """
        return _system_monitor_get_frame(self._index)

    def __repr__(self):
        return "Monitor(%r name=%r %r×%r stream=%r×%r @(%r,%r) Hz=%r)" % (
            self._index,
            self.name,
            self.width,
            self.height,
            self.stream_width,
            self.stream_height,
            self.x,
            self.y,
            self.refresh_rate,
        )


class _MonitorsSequence:
    def __len__(self):
        return _system_monitor_len()

    def __getitem__(self, i):
        if isinstance(i, slice):
            raise TypeError("monitors[:] slicing is not supported")
        n = len(self)
        if i < -n or i >= n:
            raise IndexError("monitor index out of range")
        if i < 0:
            i += n
        return Monitor(i)


monitors = _MonitorsSequence()
