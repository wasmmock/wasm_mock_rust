"""A dependency-free octos UI-Protocol client, for poking a server or the mock.

    from octos_client import Client
    c = Client(port=3341)          # or 3334 for a real `octos serve`
    c.open()                       # session/open
    c.call("loop/list", {"session_id": c.session, "profile_id": "alan"})

Raw sockets rather than the `websockets` package on purpose: that package's
handshake is refused by `octos serve` here (it returns a bare EOF), while the
same request built by hand gets a clean 101. Fewer moving parts also means the
client cannot be the reason a reply went missing, which matters when the thing
under investigation IS whether replies go missing.

`call` returns None on timeout rather than raising — "the server never answered"
is a normal, expected result against octos: loop/create, loop/pause, loop/resume
and loop/fire_now are all fire-and-forget there.
"""
import base64
import json
import os
import socket
import struct
import time

FEATURES = ("approval.typed.v1, pane.snapshots.v1, session.workspace_cwd.v1, "
            "coding.autonomy.v1, coding.agent_control.v1, coding.goal_runtime.v1, "
            "coding.loop_runtime.v1, harness.task_control.v1, state.session_hydrate.v1, "
            "user_question.v1, context.lifecycle.v1, plan.todos.v1, "
            "event.background_activity.v1, projection.envelope.v2, "
            "event.turn_steer_dropped.v1")


class WS:
    def __init__(self, host, port, path, headers, timeout=10):
        self.s = socket.create_connection((host, port), timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: Upgrade\r\n"
               f"Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n"
               f"Sec-WebSocket-Key: {key}\r\n")
        for k, v in headers.items():
            req += f"{k}: {v}\r\n"
        self.s.sendall((req + "\r\n").encode())
        self.buf = b""
        while b"\r\n\r\n" not in self.buf:
            d = self.s.recv(4096)
            if not d:
                raise RuntimeError("closed during handshake")
            self.buf += d
        head, self.buf = self.buf.split(b"\r\n\r\n", 1)
        if b"101" not in head.split(b"\r\n")[0]:
            raise RuntimeError(head.decode(errors="replace")[:200])

    def send(self, text):
        payload = text.encode()
        mask = os.urandom(4)
        n = len(payload)
        h = bytearray([0x81])
        if n < 126:
            h.append(0x80 | n)
        elif n < 65536:
            h.append(0x80 | 126); h += struct.pack(">H", n)
        else:
            h.append(0x80 | 127); h += struct.pack(">Q", n)
        h += mask
        h += bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self.s.sendall(bytes(h))

    def _need(self, n, deadline):
        while len(self.buf) < n:
            self.s.settimeout(max(0.05, deadline - time.time()))
            d = self.s.recv(65536)
            if not d:
                raise RuntimeError("closed")
            self.buf += d

    def recv(self, timeout=5):
        deadline = time.time() + timeout
        while True:
            self._need(2, deadline)
            opcode = self.buf[0] & 0x0F
            ln = self.buf[1] & 0x7F
            off = 2
            if ln == 126:
                self._need(4, deadline); ln = struct.unpack(">H", self.buf[2:4])[0]; off = 4
            elif ln == 127:
                self._need(10, deadline); ln = struct.unpack(">Q", self.buf[2:10])[0]; off = 10
            self._need(off + ln, deadline)
            payload = self.buf[off:off + ln]
            self.buf = self.buf[off + ln:]
            if opcode == 0x9:                       # ping -> pong
                self.s.sendall(b"\x8a\x80" + os.urandom(4)); continue
            if opcode == 0x8:
                raise RuntimeError("server closed")
            if opcode in (0x1, 0x2):
                return payload.decode(errors="replace")

    def close(self):
        try:
            self.s.sendall(b"\x88\x80" + os.urandom(4))
        except Exception:
            pass
        try:
            self.s.close()
        except Exception:
            pass


class Client:
    def __init__(self, port=3341, profile="alan", token="local-dev-token",
                 session="alan:local:tui#coding", cwd=None, retries=5):
        self.session = session
        self.profile = profile
        self.cwd = cwd or os.getcwd()
        last = None
        for _ in range(retries):
            try:
                self.ws = WS("127.0.0.1", port, "/api/ui-protocol/ws",
                             {"x-profile-id": profile,
                              "authorization": f"Bearer {token}",
                              "x-octos-ui-features": FEATURES})
                break
            except Exception as e:
                last = e
                time.sleep(2)
        else:
            raise RuntimeError(f"handshake failed after {retries} tries: {last}")
        self.n = 0

    def call(self, method, params, timeout=15):
        """Send and wait for the reply with the matching id. None on timeout."""
        self.n += 1
        rid = f"cli-{self.n}"
        self.ws.send(json.dumps({"jsonrpc": "2.0", "id": rid,
                                 "method": method, "params": params}))
        end = time.time() + timeout
        while time.time() < end:
            try:
                msg = self.ws.recv(2)
            except Exception:
                continue
            if json.loads(msg).get("id") == rid:
                return json.loads(msg)
        return None

    def send(self, method, params):
        """Fire and forget, for the methods octos never answers."""
        self.n += 1
        self.ws.send(json.dumps({"jsonrpc": "2.0", "id": f"cli-{self.n}",
                                 "method": method, "params": params}))

    def open(self, timeout=25):
        return self.call("session/open", {"cwd": self.cwd, "profile_id": self.profile,
                                          "session_id": self.session}, timeout)

    def loops(self):
        r = self.call("loop/list", {"session_id": self.session, "profile_id": self.profile})
        return r["result"]["loops"] if r else None
