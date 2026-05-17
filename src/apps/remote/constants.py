import xos

MESH_CHANNEL = "remote"
MODE = "lan"
USE_UDP = True  # UDP favors lower latency because it doesn't need guarenteed delivery

def get_mesh() -> xos.mesh:
    return xos.mesh.connect(id=MESH_CHANNEL, mode=MODE, udp=USE_UDP)