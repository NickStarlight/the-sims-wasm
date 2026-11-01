"""Real Chrome worker regression; requires Selenium and installed Chrome.

Run from the workspace: .tools/browser-tests/bin/python crates/runtime/tests/browser/check-scheduler.py
Optional CHROMEDRIVER and CHROME_BINARY environment variables select executables.
"""
import functools
import http.server
import json
import os
from pathlib import Path
import threading

from selenium import webdriver
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.chrome.service import Service


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


root = Path(__file__).resolve().parents[4]
server = http.server.ThreadingHTTPServer(
    ("127.0.0.1", 0), functools.partial(Handler, directory=str(root))
)
threading.Thread(target=server.serve_forever, daemon=True).start()
options = Options()
options.add_argument("--headless=new")
options.add_argument("--disable-dev-shm-usage")
if os.environ.get("CHROME_BINARY"):
    options.binary_location = os.environ["CHROME_BINARY"]
driver = None
try:
    driver = webdriver.Chrome(
        options=options, service=Service(os.environ.get("CHROMEDRIVER"))
    )
    driver.set_script_timeout(10)
    driver.get(f"http://127.0.0.1:{server.server_port}/")
    result = driver.execute_async_script("""
        const done = arguments[0];
        const source = `
          import {yieldToHost} from '${location.origin}/web/scheduler.js';
          import {schedulerSmoke} from '${location.origin}/crates/runtime/tests/browser/scheduler.mjs';
          schedulerSmoke(yieldToHost).then(
            result => postMessage({result}),
            error => postMessage({error: error.message}));
        `;
        const url = URL.createObjectURL(new Blob([source], {type: 'text/javascript'}));
        const worker = new Worker(url, {type: 'module'});
        const finish = value => {worker.terminate(); URL.revokeObjectURL(url); done(value)};
        worker.onmessage = ({data}) => finish(data);
        worker.onerror = event => finish({error: event.message});
    """)
    if "error" in result:
        raise RuntimeError(result["error"])
    print("Chrome worker scheduler fairness:", json.dumps(result["result"]))
finally:
    if driver:
        driver.quit()
    server.shutdown()
    server.server_close()
