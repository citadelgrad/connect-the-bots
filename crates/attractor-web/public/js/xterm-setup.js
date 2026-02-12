// xterm.js initialization for embedded Claude Code terminal
// Loaded via <script> tag in the shell HTML

import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';

let terminal = null;
let fitAddon = null;
let ws = null;

/**
 * Initialize xterm.js terminal in the given container element.
 * Connects to WebSocket PTY bridge at /api/terminal/ws.
 */
window.initTerminal = function(containerId) {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error('Terminal container not found:', containerId);
    return;
  }

  // Create terminal
  terminal = new Terminal({
    cursorBlink: true,
    fontSize: 14,
    fontFamily: '"SF Mono", Monaco, "Cascadia Code", "Roboto Mono", Consolas, monospace',
    theme: {
      background: '#1e1e2e',
      foreground: '#cdd6f4',
      cursor: '#f5e0dc',
      selectionBackground: '#585b70',
      black: '#45475a',
      red: '#f38ba8',
      green: '#a6e3a1',
      yellow: '#f9e2af',
      blue: '#89b4fa',
      magenta: '#f5c2e7',
      cyan: '#94e2d5',
      white: '#bac2de',
      brightBlack: '#585b70',
      brightRed: '#f38ba8',
      brightGreen: '#a6e3a1',
      brightYellow: '#f9e2af',
      brightBlue: '#89b4fa',
      brightMagenta: '#f5c2e7',
      brightCyan: '#94e2d5',
      brightWhite: '#a6adc8',
    },
    allowProposedApi: true,
  });

  fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);
  terminal.loadAddon(new WebLinksAddon());

  terminal.open(container);
  fitAddon.fit();

  // Connect WebSocket to PTY bridge
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsUrl = `${protocol}//${window.location.host}/api/terminal/ws`;
  ws = new WebSocket(wsUrl);
  ws.binaryType = 'arraybuffer';

  ws.onopen = function() {
    console.log('Terminal WebSocket connected');
    // Send initial size
    const dims = { type: 'resize', cols: terminal.cols, rows: terminal.rows };
    ws.send(JSON.stringify(dims));
  };

  ws.onmessage = function(event) {
    if (event.data instanceof ArrayBuffer) {
      terminal.write(new Uint8Array(event.data));
    } else {
      terminal.write(event.data);
    }
  };

  ws.onclose = function() {
    console.log('Terminal WebSocket closed');
    terminal.write('\r\n\x1b[31m[Connection closed]\x1b[0m\r\n');
  };

  ws.onerror = function(err) {
    console.error('Terminal WebSocket error:', err);
  };

  // Forward terminal input to WebSocket
  terminal.onData(function(data) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(new TextEncoder().encode(data));
    }
  });

  // Handle terminal resize
  terminal.onResize(function(size) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      const dims = { type: 'resize', cols: size.cols, rows: size.rows };
      ws.send(JSON.stringify(dims));
    }
  });

  // Fit on window resize
  const resizeObserver = new ResizeObserver(function() {
    if (fitAddon) {
      fitAddon.fit();
    }
  });
  resizeObserver.observe(container);

  return terminal;
};

/**
 * Dispose of terminal and close WebSocket.
 */
window.disposeTerminal = function() {
  if (ws) {
    ws.close();
    ws = null;
  }
  if (terminal) {
    terminal.dispose();
    terminal = null;
  }
};
