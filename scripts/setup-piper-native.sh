#!/bin/bash
# ============================================================================
# Piper TTS Native Setup (No Docker Required)
# Ultra low-latency local text-to-speech for Zoey
# ============================================================================

set -e

VOICE="${1:-en_US-amy-low}"
PORT="${2:-5500}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="${SCRIPT_DIR}/../crates/providers/zoey-provider-voice/voices"
PIPER_VERSION="2023.11.14-2"

echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║              PIPER TTS NATIVE SETUP (No Docker)                  ║"
echo "╠══════════════════════════════════════════════════════════════════╣"
echo "║  Voice: $VOICE"
echo "║  Port:  $PORT"
echo "║  Dir:   $INSTALL_DIR"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64)
        PIPER_ARCH="amd64"
        ;;
    aarch64|arm64)
        PIPER_ARCH="arm64"
        ;;
    *)
        echo "❌ Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

echo "📦 Architecture: $ARCH ($PIPER_ARCH)"

# Create install directory
mkdir -p "$INSTALL_DIR"
cd "$INSTALL_DIR"

# Download Piper if not present
if [ ! -f "$INSTALL_DIR/piper" ]; then
    echo "📥 Downloading Piper..."
    PIPER_URL="https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_${PIPER_ARCH}.tar.gz"
    
    if command -v wget &> /dev/null; then
        wget -q --show-progress "$PIPER_URL" -O piper.tar.gz
    elif command -v curl &> /dev/null; then
        curl -L --progress-bar "$PIPER_URL" -o piper.tar.gz
    else
        echo "❌ Neither wget nor curl found. Please install one."
        exit 1
    fi
    
    echo "📦 Extracting..."
    tar -xzf piper.tar.gz
    rm piper.tar.gz
    chmod +x piper/piper
    
    # Move binary to main dir
    mv piper/piper .
    mv piper/lib .
    rm -rf piper
    
    echo "✅ Piper installed"
else
    echo "✅ Piper already installed"
fi

# Download voice model if not present
VOICE_DIR="$INSTALL_DIR/voices"
mkdir -p "$VOICE_DIR"

# Parse voice name to construct URL
# Format: en_US-amy-low -> en/en_US/amy/low/en_US-amy-low.onnx
LANG=$(echo "$VOICE" | cut -d'-' -f1)
LANG_CODE=$(echo "$LANG" | cut -d'_' -f1)
SPEAKER=$(echo "$VOICE" | cut -d'-' -f2)
QUALITY=$(echo "$VOICE" | cut -d'-' -f3)

VOICE_FILE="$VOICE_DIR/${VOICE}.onnx"
VOICE_JSON="$VOICE_DIR/${VOICE}.onnx.json"

if [ ! -f "$VOICE_FILE" ]; then
    echo "📥 Downloading voice model: $VOICE..."
    
    BASE_URL="https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0"
    VOICE_URL="${BASE_URL}/${LANG_CODE}/${LANG}/${SPEAKER}/${QUALITY}/${VOICE}.onnx"
    JSON_URL="${BASE_URL}/${LANG_CODE}/${LANG}/${SPEAKER}/${QUALITY}/${VOICE}.onnx.json"
    
    if command -v wget &> /dev/null; then
        wget -q --show-progress "$VOICE_URL" -O "$VOICE_FILE" || {
            echo "❌ Failed to download voice. Trying alternative URL..."
            # Try alternative URL format
            VOICE_URL="https://huggingface.co/rhasspy/piper-voices/resolve/main/${LANG_CODE}/${LANG}/${SPEAKER}/${QUALITY}/${VOICE}.onnx"
            wget -q --show-progress "$VOICE_URL" -O "$VOICE_FILE"
        }
        wget -q "$JSON_URL" -O "$VOICE_JSON" 2>/dev/null || true
    else
        curl -L --progress-bar "$VOICE_URL" -o "$VOICE_FILE" || {
            echo "❌ Failed to download voice. Trying alternative URL..."
            VOICE_URL="https://huggingface.co/rhasspy/piper-voices/resolve/main/${LANG_CODE}/${LANG}/${SPEAKER}/${QUALITY}/${VOICE}.onnx"
            curl -L --progress-bar "$VOICE_URL" -o "$VOICE_FILE"
        }
        curl -sL "$JSON_URL" -o "$VOICE_JSON" 2>/dev/null || true
    fi
    
    echo "✅ Voice model downloaded"
else
    echo "✅ Voice model already present"
fi

# Create HTTP wrapper script using Python
HTTP_WRAPPER="$INSTALL_DIR/piper_http.py"
cat > "$HTTP_WRAPPER" << 'PYTHON_EOF'
#!/usr/bin/env python3
"""Simple HTTP wrapper for Piper TTS"""
import subprocess
import sys
import os
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs
import json

PIPER_PATH = os.environ.get('PIPER_PATH', './piper')
VOICE_PATH = os.environ.get('VOICE_PATH', './voices/en_US-amy-low.onnx')
PORT = int(os.environ.get('PIPER_PORT', '5500'))

class PiperHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass  # Suppress logs
    
    def do_GET(self):
        parsed = urlparse(self.path)
        
        if parsed.path == '/health' or parsed.path == '/':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(b'{"status":"ok"}')
            return
        
        if parsed.path == '/api/tts':
            params = parse_qs(parsed.query)
            text = params.get('text', [''])[0]
            if text:
                self._synthesize(text)
            else:
                self.send_response(400)
                self.end_headers()
            return
        
        self.send_response(404)
        self.end_headers()
    
    def do_POST(self):
        parsed = urlparse(self.path)
        
        if parsed.path == '/api/tts':
            content_length = int(self.headers.get('Content-Length', 0))
            body = self.rfile.read(content_length).decode('utf-8')
            
            # Try JSON first
            try:
                data = json.loads(body)
                text = data.get('text', '')
            except:
                text = body
            
            if text:
                self._synthesize(text)
            else:
                self.send_response(400)
                self.end_headers()
            return
        
        if parsed.path == '/synthesize':
            content_length = int(self.headers.get('Content-Length', 0))
            text = self.rfile.read(content_length).decode('utf-8')
            if text:
                self._synthesize(text)
            else:
                self.send_response(400)
                self.end_headers()
            return
        
        self.send_response(404)
        self.end_headers()
    
    def _synthesize(self, text):
        try:
            # Run piper and capture output
            result = subprocess.run(
                [PIPER_PATH, '--model', VOICE_PATH, '--output-raw'],
                input=text.encode('utf-8'),
                capture_output=True,
                timeout=30
            )
            
            if result.returncode == 0:
                self.send_response(200)
                self.send_header('Content-Type', 'audio/pcm')
                self.send_header('Content-Length', len(result.stdout))
                self.end_headers()
                self.wfile.write(result.stdout)
            else:
                self.send_response(500)
                self.send_header('Content-Type', 'text/plain')
                self.end_headers()
                self.wfile.write(result.stderr)
        except Exception as e:
            self.send_response(500)
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(str(e).encode())

if __name__ == '__main__':
    print(f"🎤 Piper HTTP server starting on port {PORT}...")
    print(f"   Model: {VOICE_PATH}")
    server = HTTPServer(('0.0.0.0', PORT), PiperHandler)
    print(f"✅ Ready! http://localhost:{PORT}/api/tts?text=hello")
    server.serve_forever()
PYTHON_EOF

chmod +x "$HTTP_WRAPPER"

# Create systemd service file (optional)
SERVICE_FILE="$INSTALL_DIR/piper-tts.service"
cat > "$SERVICE_FILE" << EOF
[Unit]
Description=Piper TTS HTTP Server
After=network.target

[Service]
Type=simple
Environment=PIPER_PATH=$INSTALL_DIR/piper
Environment=VOICE_PATH=$VOICE_FILE
Environment=PIPER_PORT=$PORT
Environment=LD_LIBRARY_PATH=$INSTALL_DIR/lib
ExecStart=/usr/bin/python3 $HTTP_WRAPPER
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF

echo ""
echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║                    INSTALLATION COMPLETE!                        ║"
echo "╠══════════════════════════════════════════════════════════════════╣"
echo "║                                                                  ║"
echo "║  To start Piper TTS server:                                      ║"
echo "║                                                                  ║"
echo "║  Option 1: Run directly                                          ║"
echo "║    cd $INSTALL_DIR"
echo "║    PIPER_PATH=./piper VOICE_PATH=$VOICE_FILE \\"
echo "║    LD_LIBRARY_PATH=./lib python3 piper_http.py"
echo "║                                                                  ║"
echo "║  Option 2: Use the start script below                            ║"
echo "║    $INSTALL_DIR/start-piper.sh"
echo "║                                                                  ║"
echo "║  Option 3: Install as systemd service                            ║"
echo "║    sudo cp $SERVICE_FILE /etc/systemd/system/"
echo "║    sudo systemctl enable --now piper-tts"
echo "║                                                                  ║"
echo "╚══════════════════════════════════════════════════════════════════╝"

# Create convenience start script
START_SCRIPT="$INSTALL_DIR/start-piper.sh"
cat > "$START_SCRIPT" << EOF
#!/bin/bash
cd "$INSTALL_DIR"
export PIPER_PATH="$INSTALL_DIR/piper"
export VOICE_PATH="$VOICE_FILE"
export PIPER_PORT="$PORT"
export LD_LIBRARY_PATH="$INSTALL_DIR/lib:\$LD_LIBRARY_PATH"
python3 piper_http.py
EOF
chmod +x "$START_SCRIPT"

echo ""
echo "🚀 Starting Piper TTS server now..."
echo ""

# Start the server
cd "$INSTALL_DIR"
export PIPER_PATH="$INSTALL_DIR/piper"
export VOICE_PATH="$VOICE_FILE"
export PIPER_PORT="$PORT"
export LD_LIBRARY_PATH="$INSTALL_DIR/lib:$LD_LIBRARY_PATH"

# Check if Python 3 is available
if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 not found. Please install: apt install python3"
    exit 1
fi

# Run in background
nohup python3 piper_http.py > piper.log 2>&1 &
PIPER_PID=$!

sleep 2

# Check if running
if kill -0 $PIPER_PID 2>/dev/null; then
    echo "✅ Piper TTS server running on http://localhost:$PORT"
    echo "   PID: $PIPER_PID"
    echo ""
    echo "   Test it: curl 'http://localhost:$PORT/api/tts?text=hello' | aplay -r 22050 -f S16_LE"
    echo ""
    echo "   Stop it: kill $PIPER_PID"
    echo "   Logs:    tail -f $INSTALL_DIR/piper.log"
else
    echo "❌ Server failed to start. Check logs:"
    cat piper.log
fi

