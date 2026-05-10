"""
LAN remote desktop viewer — pairs with the macOS daemon (`xos on`) on mesh ``xos-remote``.

Requirements: logged in on both sides (`xos login`), same LAN, daemon running on Mac.
Tap a desktop host row, control it from the viewport above the on-screen keyboard.
"""

import xos

import devices

MESH_ID = "xos-remote"
MODE = "lan"

KIND_FRAME = "remote_frame"
KIND_INPUT = "remote_input"
KIND_PEER = "remote_peer"

MAX_ROWS = 6


def _clamp01(value):
    v = float(value)
    return 0.0 if v < 0.0 else 1.0 if v > 1.0 else v


def other_rank(mesh):
    return 1 - int(mesh.rank())


def _norm_pointer(mouse_x, mouse_y, fit):
    fx, fy, fw, fh = fit
    if fw < 1.0 or fh < 1.0:
        return 0.5, 0.5
    nx = _clamp01((float(mouse_x) - fx) / fw)
    ny = _clamp01((float(mouse_y) - fy) / fh)
    return nx, ny


class RemoteApp(xos.Application):

    def __init__(self):
        super().__init__()
        self.mode = "pick"
        self._pending_scroll = 0.0
        self._last_fit = (0.0, 0.0, 1.0, 1.0)
        self._peers = {}
        self.mesh = xos.mesh.connect(id=MESH_ID, mode=MODE)

        self.keyboard = xos.ui.onscreen_keyboard()
        self.viewport = xos.ui.video(0.0, 0.0, 1.0, 1.0)

        self._hint = devices.status_line(self._idle_hint())
        self._pick_layout_dirty = True
        self._pick_group = xos.ui.group(self._hint)

        self._view_nav = xos.ui.group(devices.back_button(self._leave_view))

    def _mine(self):
        try:
            return str(self.mesh.node_id())
        except Exception:
            return ""

    def _idle_hint(self):
        return "[Remote] " + self.mesh.prompt().strip()

    def _fresh_mesh(self):
        xos.mesh.disconnect()
        self.mesh = xos.mesh.connect(id=MESH_ID, mode=MODE)
        self._peers.clear()
        self._pick_layout_dirty = True
        self._hint.text = self._idle_hint()

    def _leave_view(self):
        self.mode = "pick"
        self._pick_layout_dirty = True
        self._hint.text = self._idle_hint()

    def _enter_view(self):
        self.mode = "view"

    def _poll_peers(self):
        batch = self.mesh.receive(id=KIND_PEER, wait=False, latest_only=False)
        if batch is None:
            return

        mine = self._mine()
        touched = False
        for pkt in batch:
            node_id = str(getattr(pkt, "node_id", "") or "").strip()
            label = str(getattr(pkt, "name", "") or "").strip()
            if len(node_id) < 8:
                continue
            if node_id == mine:
                continue
            if not label:
                label = node_id[:10]
            if self._peers.get(node_id) != label:
                self._peers[node_id] = label
                touched = True
        if touched and self.mode == "pick":
            self._pick_layout_dirty = True

    def _other_hosts(self):
        mine = self._mine()
        rows = [(nid, title) for nid, title in self._peers.items() if nid and nid != mine]
        rows.sort(key=lambda nt: (nt[1].lower(), nt[0]))
        return rows

    def _maybe_relayout_pick(self):
        if self.mode != "pick" or not self._pick_layout_dirty:
            return
        self._pick_layout_dirty = False

        others = len(self._other_hosts())
        self._hint.text = (
            "[Remote] {} reachable desktop(s)".format(others) if others else self._idle_hint()
        )

        children = [self._hint, devices.reconnect_button(self._fresh_mesh)]
        hosts = self._other_hosts()

        if not hosts:
            children.append(
                xos.ui.text(
                    "Run `xos on` on the Mac · same Wi‑Fi",
                    x1=0.05,
                    y1=0.85,
                    x2=0.95,
                    y2=0.98,
                    editable=False,
                    selectable=False,
                    scrollable=False,
                    show_cursor=False,
                    size=16.0,
                    color=xos.color.GRAY,
                    alignment=(0.5, 0.5),
                )
            )
        else:
            base_y = 0.12
            step = 0.086
            for idx, (nid, title) in enumerate(hosts[:MAX_ROWS]):
                y0 = base_y + idx * step
                y1 = y0 + step * 0.88
                lbl = xos.ui.text(
                    title,
                    x1=0.12,
                    y1=y0,
                    x2=0.92,
                    y2=y1,
                    editable=False,
                    selectable=False,
                    scrollable=False,
                    show_cursor=False,
                    size=23.0,
                    alignment=(0.0, 0.5),
                )
                hit = xos.ui.button(
                    0.04,
                    y0,
                    0.98,
                    y1,
                    on_press=self._tap_host(nid),
                )
                children.extend((lbl, hit))

        self._pick_group = xos.ui.group(*children)

    def _tap_host(self, node_id):

        def go():
            self._focus = node_id
            self._enter_view()

        return go

    def tick(self):
        self.keyboard.tick(self)
        self._poll_peers()
        self._maybe_relayout_pick()

        nodes = int(self.mesh.num_nodes())

        if self.mode == "pick":
            self.frame.clear((12, 12, 16, 255))
            self._pick_group.tick(self)
            self._pick_group.render(self)
            if self.t % 240 == 0:
                print("[remote]", self.mesh.prompt().strip(), "peers=", len(self._peers))
            return

        self.frame.clear(xos.color.BLACK)

        ky = float(getattr(self.keyboard, "y1", 0.88))
        self.viewport.x1 = 0.0
        self.viewport.x2 = 1.0
        self.viewport.y1 = 0.0
        self.viewport.y2 = ky

        pkt = None
        if nodes >= 2:
            pkt = self.mesh.receive(KIND_FRAME, wait=False, latest_only=True)

        if pkt is not None:
            self.viewport.blit(self, pkt)

        fx, fy, fv_w, fv_h = self.viewport.last_fit
        fw = float(self.frame.get_width())
        fh = float(self.frame.get_height())
        if fv_w != fv_w or fv_h != fv_h:
            self._last_fit = (0.0, 0.0, fw, fh)
        else:
            self._last_fit = (fx, fy, fv_w, fv_h)

        self._push_input(nodes)

        self._view_nav.tick(self)
        self._view_nav.render(self)
        self.keyboard.render(self)

    def _push_input(self, nodes: int):
        if nodes < 2:
            self._pending_scroll = 0.0
            return
        nx, ny = _norm_pointer(self.mouse["x"], self.mouse["y"], self._last_fit)
        peer = other_rank(self.mesh)
        scroll = float(self._pending_scroll)
        self._pending_scroll = 0.0
        self.mesh.send(
            id=KIND_INPUT,
            to=peer,
            nx=nx,
            ny=ny,
            left=bool(self.mouse["is_left_clicking"]),
            right=bool(self.mouse.get("is_right_clicking", False)),
            scroll=scroll,
        )

    def on_mouse_move(self, _x, _y):
        if self.mode == "view":
            self._push_input(int(self.mesh.num_nodes()))

    def on_mouse_down(self, _x, _y):
        if self.mode == "view":
            self._push_input(int(self.mesh.num_nodes()))

    def on_mouse_up(self, _x, _y):
        if self.mode == "view":
            self._push_input(int(self.mesh.num_nodes()))

    def on_scroll(self, _dx, dy):
        if self.mode == "view":
            self._pending_scroll += float(dy)
            self._push_input(int(self.mesh.num_nodes()))

    def on_events(self):
        if self.mode == "pick":
            self._pick_group.on_events(self)
        else:
            self.keyboard.on_events(self)
            self._view_nav.on_events(self)


def main():
    RemoteApp().run()


if __name__ == "__main__":
    main()
