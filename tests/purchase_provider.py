"""Shared loopback PayPal/renderer fixture for contract and recovery tests."""

import base64
import copy
import datetime
import http.server
import json
import time
import uuid


def provider_handler(orders, rendered, controls, paid_event, release_event):
    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def reply(self, status, payload):
            body = json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def do_GET(self):
            assert self.headers.get("Authorization") == "Bearer synthetic-provider"
            order = orders[self.path.split("/")[-1]]
            if order.get("get_fail"):
                return self.reply(503, {})
            self.reply(200, order["remote"])

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
            if self.path == "/v1/oauth2/token":
                assert (
                    self.headers.get("Authorization")
                    == "Basic " + base64.b64encode(b"test-client:test-secret").decode()
                )
                assert body == b"grant_type=client_credentials"
                return self.reply(200, {"access_token": "synthetic-provider", "token_type": "Bearer"})
            if self.path == "/html_to_pdf":
                rendered.append(body.decode())
                if controls["render_fail"]:
                    return self.reply(503, {})
                # Synthetic bytes intentionally avoid requiring a browser or external renderer.
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"%PDF-SYNTHETIC\n" + body + controls.get("render_marker", b""))
                return
            assert self.headers.get("Authorization") == "Bearer synthetic-provider"
            if self.path == "/v2/checkout/orders":
                data = json.loads(body)
                oid = uuid.uuid4().hex.upper()
                orders[oid] = {
                    "remote": {
                        "id": oid,
                        "intent": "CAPTURE",
                        "status": "CREATED",
                        "purchase_units": [
                            {
                                "amount": data["purchase_units"][0]["amount"],
                                "payee": {"merchant_id": "SYNTHETICMERCHANT"},
                                "payments": {"captures": []},
                            }
                        ],
                    },
                    "calls": [],
                    "charges": 0,
                }
                return self.reply(201, {"id": oid})
            oid = self.path.split("/")[-2]
            order = orders[oid]
            key = self.headers.get("PayPal-Request-Id")
            assert key and str(uuid.UUID(key)) == key
            assert self.headers.get("Prefer") == "return=representation"
            order["calls"].append(key)
            mode = order.get("mode")
            if mode == "fail_before":
                return self.reply(503, {})
            remote = order["remote"]
            unit = remote["purchase_units"][0]
            if not unit["payments"]["captures"]:
                remote["status"] = "COMPLETED"
                unit["payments"]["captures"] = [
                    {
                        "id": "CAP" + oid,
                        "status": "PENDING" if mode == "pending" else "COMPLETED",
                        "amount": copy.deepcopy(unit["amount"]),
                        "create_time": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    }
                ]
                order["charges"] += 1
            if mode == "lost_unknown":
                order["get_fail"] = True
            if mode == "timeout":
                time.sleep(31)  # Exceeds the real provider adapter's 30-second request deadline.
            if mode == "crash":
                paid_event.set()
                release_event.wait(40)
            if mode in ("lost", "lost_unknown"):
                self.close_connection = True
                return
            if mode == "already":
                return self.reply(422, {"details": [{"issue": "ORDER_ALREADY_CAPTURED"}]})
            self.reply(201, remote)

    return Provider
