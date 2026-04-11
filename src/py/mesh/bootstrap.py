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
    def rank(self):
        return _mesh_rank()

    def num_nodes(self):
        return _mesh_num_nodes()

    def broadcast(self, **kwargs):
        kind = kwargs.pop("id")
        _mesh_broadcast_payload(kind, kwargs)

    def send(self, to=None, **kwargs):
        kind = kwargs.pop("id")
        _mesh_send_payload(kind, to, kwargs)

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
        r = _mesh_receive(kind, wait, latest_only)
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


def connect(id="default", mode="local"):
    """Join a mesh. ``id`` selects the logical room (TCP + UDP discovery ports). ``mode`` is
    ``local``, ``lan``, or ``online`` (``online`` raises until implemented). For ``lan``, run
    ``xos login --offline`` first so ``identity.json`` holds your RSA keys; connect loads them
    from disk (no password prompt).
    """
    mode = (mode or "local").lower()
    if mode == "online":
        raise NotImplementedError("xos.mesh mode 'online' is not implemented yet")
    if mode not in ("local", "lan"):
        raise ValueError("xos.mesh.connect: mode must be 'local', 'lan', or 'online'")
    _mesh_connect(id, mode)
    return Mesh()
