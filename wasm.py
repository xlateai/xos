#!/usr/bin/env python
import os
import subprocess
import base64
import webbrowser
from pathlib import Path
import uvicorn
from fastapi import FastAPI
from fastapi.responses import HTMLResponse
import argparse

app = FastAPI()

# Paths
CARGO_TOML = Path("Cargo.toml")
WASM_TARGET = "wasm32-unknown-unknown"
RELEASE_DIR = Path(f"target/{WASM_TARGET}/release")

def get_crate_name():
    """Extract crate name from Cargo.toml."""
    with open(CARGO_TOML, "r") as f:
        for line in f:
            if line.strip().startswith("name"):
                parts = line.split("=")
                if len(parts) == 2:
                    return parts[1].strip().strip('"').strip("'")
    raise ValueError("Could not find crate name in Cargo.toml")

def build_wasm():
    """Build the WebAssembly binary."""
    print("Building WASM target...")
    result = subprocess.run(
        ["cargo", "build", "--lib", "--target", WASM_TARGET, "--release"],
        capture_output=True,
        text=True
    )
    
    if result.returncode != 0:
        print("Failed to build WASM target:")
        print(result.stderr)
        return False
    
    print("WASM build successful!")
    return True

def get_base64_wasm(wasm_path):
    """Read WASM file and convert to base64."""
    with open(wasm_path, "rb") as f:
        wasm_bytes = f.read()
    return base64.b64encode(wasm_bytes).decode("utf-8")

def create_html_template(base64_wasm):
    """Create HTML template with embedded WASM."""
    return f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>XOS WASM Viewport</title>
    <style>
        body {{ margin: 0; padding: 0; background-color: #222; display: flex; justify-content: center; align-items: center; height: 100vh; }}
        canvas {{ display: block; }}
    </style>
</head>
<body>
    <canvas id="canvas" width="320" height="240"></canvas>

    <script>
        // The base64-encoded WebAssembly module
        const wasmBase64 = "{base64_wasm}";
        
        // Convert base64 to binary
        const wasmBytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0));
        
        // Canvas setup
        const canvas = document.getElementById('canvas');
        const ctx = canvas.getContext('2d');
        const imageData = ctx.createImageData(canvas.width, canvas.height);
        
        // The memory we'll share with WASM
        let memory;
        
        // WASM instance
        WebAssembly.instantiate(wasmBytes, {{
            env: {{
                // Provide a function for WASM to call to update the display
                update_display: (ptr, width, height) => {{
                    const buffer = new Uint8Array(memory.buffer, ptr, width * height * 4);
                    const data = new Uint8ClampedArray(buffer);
                    imageData.data.set(data);
                    ctx.putImageData(imageData, 0, 0);
                }}
            }}
        }}).then(result => {{
            const instance = result.instance;
            memory = instance.exports.memory;
            
            // Call the init function if it exists
            if (instance.exports.init) {{
                instance.exports.init();
            }}
            
            // Call the animation loop function if it exists
            function animate() {{
                if (instance.exports.update) {{
                    instance.exports.update();
                }}
                requestAnimationFrame(animate);
            }}
            
            animate();
        }}).catch(e => {{
            console.error("WASM instantiation failed:", e);
        }});
    </script>
</body>
</html>"""

# Define the route to serve the WASM app
@app.get("/", response_class=HTMLResponse)
async def serve_wasm_app():
    crate_name = get_crate_name()
    wasm_path = RELEASE_DIR / f"{crate_name}.wasm"
    
    # Check if WASM file exists, build if necessary
    if not wasm_path.exists() or not wasm_path.is_file():
        if not build_wasm():
            return HTMLResponse(content="<h1>Error building WASM</h1>", status_code=500)
    
    # Get base64 WASM and create HTML
    base64_wasm = get_base64_wasm(wasm_path)
    html_content = create_html_template(base64_wasm)
    
    return HTMLResponse(content=html_content)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Serve Rust WASM application")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind server to")
    parser.add_argument("--port", type=int, default=8000, help="Port to bind server to")
    parser.add_argument("--no-open", action="store_true", help="Don't open browser automatically")
    args = parser.parse_args()
    
    # Ensure WASM target is installed
    subprocess.run(["rustup", "target", "add", WASM_TARGET], 
                   capture_output=True, check=False)
    
    # Build WASM before starting server
    build_wasm()
    
    # Open browser
    if not args.no_open:
        webbrowser.open(f"http://{args.host}:{args.port}")
    
    # Start server
    print(f"Starting server at http://{args.host}:{args.port}")
    uvicorn.run(app, host=args.host, port=args.port)