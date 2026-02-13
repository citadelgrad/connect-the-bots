// xterm.js terminal initialization — loaded as ES module from CDN
import { Terminal } from 'https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/+esm';
import { FitAddon } from 'https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/+esm';
import { WebLinksAddon } from 'https://cdn.jsdelivr.net/npm/@xterm/addon-web-links@0.11.0/+esm';

window.initTerminal = function(containerId) {
    const container = document.getElementById(containerId);
    if (!container) {
        console.error('Terminal container not found:', containerId);
        return;
    }

    const terminal = new Terminal({
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

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());
    terminal.open(container);
    fitAddon.fit();

    let ws = null;
    let reconnectTimer = null;
    const RECONNECT_DELAY = 1000;

    function getSessionId() {
        return sessionStorage.getItem('terminal_session_id');
    }

    function setSessionId(id) {
        sessionStorage.setItem('terminal_session_id', id);
    }

    function buildWsUrl() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let url = protocol + '//' + window.location.host + '/api/terminal/ws';
        const sid = getSessionId();
        if (sid) {
            url += '?session=' + encodeURIComponent(sid);
        }
        return url;
    }

    function connect() {
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }

        const wsUrl = buildWsUrl();
        ws = new WebSocket(wsUrl);
        ws.binaryType = 'arraybuffer';

        ws.onopen = function() {
            ws.send(JSON.stringify({ type: 'resize', cols: terminal.cols, rows: terminal.rows }));
        };

        ws.onmessage = function(event) {
            if (event.data instanceof ArrayBuffer) {
                terminal.write(new Uint8Array(event.data));
            } else {
                // Check for session protocol message
                try {
                    const msg = JSON.parse(event.data);
                    if (msg.type === 'session' && msg.session_id) {
                        setSessionId(msg.session_id);
                        console.log('Terminal session:', msg.session_id);
                        return;
                    }
                } catch (_) {
                    // Not JSON — regular terminal output
                }
                terminal.write(event.data);
            }
        };

        ws.onclose = function() {
            terminal.write('\r\n\x1b[33m[Reconnecting...]\x1b[0m\r\n');
            scheduleReconnect();
        };

        ws.onerror = function(err) {
            console.error('Terminal WebSocket error:', err);
        };
    }

    function scheduleReconnect() {
        if (!reconnectTimer) {
            reconnectTimer = setTimeout(connect, RECONNECT_DELAY);
        }
    }

    terminal.onData(function(data) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(new TextEncoder().encode(data));
        }
    });
    terminal.onResize(function(size) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: 'resize', cols: size.cols, rows: size.rows }));
        }
    });

    new ResizeObserver(function() { fitAddon.fit(); }).observe(container);

    connect();
    console.log('Terminal initialized');
};
