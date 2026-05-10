# Loaded by Rust (`include_str!`) into `xos.mesh`. Native hooks: `_mesh_*`.


class _Packet:
    """Dot-access view of a dict from Rust (RustPython-friendly; no `types` import)."""

    def __init__(self, d):
        self.__dict__.update(d)


def _wrap_receive_result(r):
    if r is None:
        return None

    def wrap(obj):
        if isinstance(obj, dict):
            return _Packet(obj)
        return obj

    if isinstance(r, list):
        return [wrap(x) for x in r]
    return wrap(r)


class Mesh:
    def __init__(self, mesh_id, mode, udp=False):
        self._mesh_id = mesh_id
        self._mode = mode
        self._udp = bool(udp)

    def _ensure_connected(self):
        if _mesh_is_connected():
            return
        _mesh_connect(self._mesh_id, self._mode, self._udp)

    def _call(self, fn, *args):
        self._ensure_connected()
        try:
            return fn(*args)
        except Exception:
            # If coordinator changed between pre-check and call, reconnect and retry once.
            self._ensure_connected()
            return fn(*args)

    def rank(self):
        return self._call(_mesh_rank)

    def num_nodes(self):
        return self._call(_mesh_num_nodes)

    def node_id(self):
        """This session’s stable node id (64-char hex = SHA256 of node public key)."""
        return self._call(_mesh_node_id)

    def node_name(self):
        """Friendly machine name from offline identity (`node_identity.json`)."""
        return self._call(_mesh_node_name)

    def prompt(self):
        """Input prompt prefix with live ``n=`` / ``rank=`` (call each loop iteration)."""
        return self._call(_mesh_prompt)

    def disconnect(self):
        """Drop the singleton mesh session (next ``connect`` attaches fresh)."""
        _mesh_disconnect()

    def broadcast(self, **kwargs):
        kind = kwargs.pop("id")
        self._call(_mesh_broadcast_payload, kind, kwargs)

    def send(self, to=None, **kwargs):
        kind = kwargs.pop("id")
        self._call(_mesh_send_payload, kind, to, kwargs)

    def receive(self, *args, **kwargs):
        wait = kwargs.pop("wait", True)
        latest_only = kwargs.pop("latest_only", False)
        if "id" in kwargs:
            kind = kwargs.pop("id")
        elif len(args) >= 1:
            kind = args[0]
        else:
            raise TypeError("mesh.receive() requires a message id (positional or id=...)")
        if len(args) >= 2:
            wait = args[1]
        if len(args) >= 3:
            latest_only = args[2]
        if kwargs:
            raise TypeError(
                "receive() got unexpected keyword arguments: %s" % (tuple(kwargs.keys()),)
            )
        r = self._call(_mesh_receive, kind, wait, latest_only)
        return _wrap_receive_result(r)

    def node(self, rank):
        return _MeshNode(self, rank)


class _MeshNode:
    def __init__(self, mesh, rank):
        self._mesh = mesh
        self._rank = rank

    def send(self, **kwargs):
        kind = kwargs.pop("id")
        self._mesh.send(id=kind, to=self._rank, **kwargs)


def disconnect():
    """Clear the singleton mesh session (fresh join on the next ``connect``)."""
    _mesh_disconnect()


def connect(id="default", mode="local", udp=False):
    """Join a mesh. ``id`` selects the logical room (TCP + UDP discovery ports). ``mode`` is
    ``local``, ``lan``, or ``online``. For ``lan``/``online``, run
    ``xos login --offline`` first so ``authentication.json`` and ``node_identity.json`` exist;
    both use the per-machine node keypair from ``node_identity.json`` (no password prompt).

    ``udp``: when True (``lan`` only), the join handshake requires every node to agree; both
    coordinator and joiners must pass ``udp=True``. Payload transport still uses the existing
    encrypted TCP channel until a separate UDP data plane is enabled.

    ``broadcast()`` posts to a background coalescing queue (latest payload wins per message id) so
    the main loop does not wait on TCP writes; ``send(..., to=...)`` stays synchronous.
    """
    mode = (mode or "local").lower()
    if mode not in ("local", "lan", "online"):
        raise ValueError("xos.mesh.connect: mode must be 'local', 'lan', or 'online'")
    udp = bool(udp)
    if udp and mode != "lan":
        raise ValueError("xos.mesh.connect: udp=True is only supported for mode='lan'")
    _mesh_connect(id, mode, udp)
    return Mesh(id, mode, udp)
