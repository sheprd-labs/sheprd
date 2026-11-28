#!/usr/bin/env python3
"""Local HTTPS/WSS browser acceptance. Requires Playwright Chromium and openssl."""
import argparse
import asyncio
import base64
import json
import os
from pathlib import Path
import ssl
import subprocess
import tempfile

from playwright.async_api import async_playwright


async def wait_file(path, proc):
    while not path.exists():
        if proc.returncode is not None:
            raise RuntimeError("synthetic host exited")
        await asyncio.sleep(.05)
    return path.read_text()


async def exercise(page, ctx, dir, proc, origin, shots):
    async def state(name):
        await page.locator(f'body[data-state="{name}"]').wait_for(timeout=15000)
        if shots:
            await page.screenshot(path=shots / f"{name}.png")

    async def visit(url):
        # Avoid Playwright call logs containing the synthetic bearer on failure.
        await page.goto(url)
        await state("join")
        assert page.url == origin + "/share/"

    async def join():
        await page.get_by_role("button", name="Join session").click()

    link = await wait_file(dir / "revocation", proc)
    requests = []
    ctx.on("request", lambda req: requests.append(req.url))
    await visit(link)
    assert all("/v1/" not in url and "#" not in url for url in requests)
    assert "Can you fix" not in await page.locator("body").inner_text()
    # Hold the static worker response to capture the genuine connecting state.
    gate = asyncio.Event()

    async def hold(route):
        await gate.wait()
        await route.continue_()

    await ctx.route("**/worker.js", hold)
    await join()
    await state("connecting")
    gate.set()
    await state("live")
    await ctx.unroute("**/worker.js", hold)
    if shots:
        await page.set_viewport_size({"width":390,"height":844})
        await page.screenshot(path=shots / "live-narrow.png")
        assert await page.evaluate("document.documentElement.scrollWidth <= innerWidth")
        await page.set_viewport_size({"width":1120,"height":720})
    body = await page.locator("body").inner_text()
    assert "Can you fix" in body and "Checked cursor guard" in body
    assert "Internal chain" not in body and "/private" not in body
    (dir / "update").touch()
    await page.get_by_text("Regression test passes.", exact=True).wait_for()
    assert await page.locator("article img, article a").count() == 0
    assert all(url.startswith(origin + "/") for url in requests)
    (dir / "finish-revocation").touch()
    await state("reconnecting")
    await state("withdrawn")
    assert not await page.locator("article").count()
    await visit(link)
    await join()
    await state("offline")
    ended = await wait_file(dir / "ended", proc)
    await visit(ended)
    await join()
    await state("live")
    (dir / "end-session").touch()
    await state("ended")
    (dir / "finish-ended").touch()
    expiry = await wait_file(dir / "expiry", proc)
    await visit(expiry)
    await join()
    await state("live")
    await state("expired")
    envelope = json.loads(base64.urlsafe_b64decode(link.split("#")[1] + "==="))
    envelope["version"] += 1
    fragment = base64.urlsafe_b64encode(json.dumps(envelope).encode()).rstrip(b"=").decode()
    await visit(origin + "/share/#" + fragment)
    await join()
    await state("version")
    await page.goto(origin + "/share/")
    await state("invalid")
    (dir / "finish-expiry").touch()


async def run(dir, shots):
    # TLS termination forwards the original HTTP stream, including WebSocket upgrades.
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    cert, key = dir / "cert", dir / "key"
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                    "-subj", "/CN=localhost", "-days", "1", "-out", cert, "-keyout", key],
                   check=True, stderr=subprocess.DEVNULL)
    tls.load_cert_chain(cert, key)

    async def proxy(reader, writer):
        upstream, target = await asyncio.open_connection("127.0.0.1", int((dir / "http").read_text()))

        async def forward(src, dst):
            try:
                while data := await src.read(65536):
                    dst.write(data)
                    await dst.drain()
            except (ConnectionError, ssl.SSLError):
                pass
            finally:
                dst.close()

        await asyncio.gather(forward(reader, target), forward(upstream, writer))

    async with await asyncio.start_server(proxy, "127.0.0.1", 0, ssl=tls) as server:
        port = server.sockets[0].getsockname()[1]
        env = dict(os.environ, SHEPRD_VIEWER_PORT=str(port), SHEPRD_VIEWER_FIXTURE=str(dir))
        proc = await asyncio.create_subprocess_exec("cargo", "test", "--locked", "-p",
            "sheprd-remote-control", "--lib", "browser_share_fixture", "--", "--ignored", env=env)
        try:
            async with async_playwright() as pw:
                browser = await pw.chromium.launch(executable_path=os.getenv("CHROMIUM"))
                ctx = await browser.new_context(ignore_https_errors=True, viewport={"width":1120,"height":720})
                await exercise(await ctx.new_page(), ctx, dir, proc, f"https://127.0.0.1:{port}/relay", shots)
                await browser.close()
            assert await proc.wait() == 0
        finally:
            if proc.returncode is None:
                # Release any pending fixture coordination before waiting for the child.
                for name in ["update","end-session","finish-revocation","finish-ended","finish-expiry"]:
                    (dir / name).touch()
                await proc.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--screenshots", type=Path)
    args = parser.parse_args()
    if args.screenshots:
        args.screenshots.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temporary:
        asyncio.run(run(Path(temporary), args.screenshots))
