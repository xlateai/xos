import xos

ID = "remote"
# "local" = plaintext, same-machine. "lan" = encrypted + discovery (also tries loopback first).
MODE = "lan"



class RemoteApp(xos.Application):
    def __init__(self):
        self.mesh = xos.mesh.connect(id=ID, mode=MODE)
        super().__init__(headless=self.rank)

    @xos.cached_property  # TODO: needs implementation
    def rank(self):
        return self.mesh.rank()

    def tick(self):
        if self.rank == 0:
            self.tick_viewer(state)
        else:
            self.tick_streamer(state)

    def tick_viewer(self):
        # we just render the frame that is sent from rank 1 to rank 0
        stream_frame = self.mesh.receive(id="stream_frame", wait=False, latest_only=True)
        if stream_frame is not None:
            xos.rasterizer.frame_in_frame(self.frame, stream_frame)

        # we only send the input to rank 1 (broadcast would work but we might upgrade this to >2 nodes)
        self.mesh.send(id="input", rank=1, mouse=self.mouse)

    def tick_streamer(self):
        # streams the frame without copying into python since we just pass the screen reference
        self.mesh.send(id="stream_frame", rank=0, stream_frame=self.screen)
        mouse_input = self.mesh.receive(id="mouse_input", wait=False, latest_only=True)

        if xos.mouse is None:
            raise ValueError("This machine does not have a mouse.")

        if mouse_input is not None:
            xos.mouse.set_position(mouse_input.x, mouse_input.y)

            if mouse_input.is_left_clicking:
                xos.mouse.left_click()
            if mouse_input.is_right_clicking:
                xos.mouse.right_click()
            if mouse_input.scroll_y:
                xos.mouse.scroll_y(mouse_input.scroll)


def main() -> None:
    mesh = xos.mesh.connect(id=ID, mode=MODE)
    print(
        f"[mesh] rank={mesh.rank()}  nodes={mesh.num_nodes()}  "
        f"id={CHAT_ID!r}  mode={MODE!r}"
    )
    print("Type a line and press Enter to broadcast. Ctrl+C to exit.\n")

    while True:
        # Drain inbound first so chat updates without waiting for local input.
        packets = mesh.receive(id="message", wait=False, latest_only=False)
        if packets:
            for packet in packets:
                sid = getattr(packet, "from_id", "") or ""
                short = (sid[:8] + "…") if len(sid) >= 8 else (sid or "?")
                text = getattr(packet, "msg", "")
                print(f"[rank {getattr(packet, 'from_rank', '?')} id {short}] {text}")

        line = xos.input(mesh.prompt(), wait=False)
        if line is not None:
            mesh.broadcast(id="message", msg=line)


main()
