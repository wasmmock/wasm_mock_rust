"""WebSocket tap: listen on LISTEN_PORT, relay to UPSTREAM, log every text frame.

Unlike the wasm tcp_fiddler this pairs replies with their request by JSON-RPC
id rather than by whatever was in flight, and it never drops a frame.

  python3 wstap.py 3336 3334 out.jsonl
"""
import json, os, socket, struct, sys, threading, time

LISTEN = int(sys.argv[1]) if len(sys.argv) > 1 else 3336
UPSTREAM = int(sys.argv[2]) if len(sys.argv) > 2 else 3334
OUT = sys.argv[3] if len(sys.argv) > 3 else "wstap.jsonl"

lock = threading.Lock()


def log(rec):
    with lock:
        with open(OUT, "a") as f:
            f.write(json.dumps(rec) + "\n")


class Framer:
    """Incremental WS frame parser; yields reassembled text messages."""

    def __init__(self):
        self.buf = bytearray()
        self.frag = bytearray()
        self.frag_op = None
        self.upgraded = False

    def feed(self, data):
        self.buf += data
        out = []
        if not self.upgraded:
            # The HTTP upgrade is not a frame; drop it before parsing anything.
            i = self.buf.find(b"\r\n\r\n")
            if i < 0:
                return out
            del self.buf[:i + 4]
            self.upgraded = True
        while True:
            b = self.buf
            if len(b) < 2:
                break
            fin = b[0] & 0x80
            opcode = b[0] & 0x0F
            masked = b[1] & 0x80
            ln = b[1] & 0x7F
            off = 2
            if ln == 126:
                if len(b) < off + 2:
                    break
                ln = struct.unpack(">H", bytes(b[off:off + 2]))[0]
                off += 2
            elif ln == 127:
                if len(b) < off + 8:
                    break
                ln = struct.unpack(">Q", bytes(b[off:off + 8]))[0]
                off += 8
            if masked:
                if len(b) < off + 4:
                    break
                mask = bytes(b[off:off + 4])
                off += 4
            else:
                mask = None
            if len(b) < off + ln:
                break
            payload = bytes(b[off:off + ln])
            if mask:
                payload = bytes(c ^ mask[i % 4] for i, c in enumerate(payload))
            del self.buf[:off + ln]

            if opcode == 0x0:            # continuation
                self.frag += payload
                if fin and self.frag_op == 0x1:
                    out.append(bytes(self.frag)); self.frag = bytearray(); self.frag_op = None
            elif opcode in (0x1, 0x2):
                if not fin:
                    self.frag = bytearray(payload); self.frag_op = opcode
                elif opcode == 0x1:
                    out.append(payload)
        return out


def pump(src, dst, direction, framer):
    try:
        while True:
            data = src.recv(65536)
            if not data:
                break
            dst.sendall(data)
            for msg in framer.feed(data):
                try:
                    text = msg.decode()
                except UnicodeDecodeError:
                    continue
                rec = {"t": round(time.time(), 3), "dir": direction, "raw": text}
                try:
                    j = json.loads(text)
                    rec["id"] = j.get("id")
                    rec["method"] = j.get("method")
                    rec["kind"] = ("request" if "method" in j and "id" in j
                                   else "notification" if "method" in j
                                   else "response")
                except Exception:
                    pass
                log(rec)
    except Exception:
        pass
    finally:
        for s in (src, dst):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except Exception:
                pass
            try:
                s.close()
            except Exception:
                pass


def handle(client):
    up = socket.create_connection(("127.0.0.1", UPSTREAM), 10)
    # Relay the HTTP upgrade verbatim; frame parsing starts after the headers.
    threading.Thread(target=pump, args=(client, up, "c2s", Framer()), daemon=True).start()
    threading.Thread(target=pump, args=(up, client, "s2c", Framer()), daemon=True).start()


srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", LISTEN))
srv.listen(16)
print(f"wstap {LISTEN} -> {UPSTREAM}, logging to {OUT}", flush=True)
while True:
    c, _ = srv.accept()
    threading.Thread(target=handle, args=(c,), daemon=True).start()
