import UI from './app/ui.js';

// The stock Connect button starts the browser before opening its WebSocket.
// Everything after startup, including phone input, stays in noVNC's UI.
const connect = UI.connect;
let launching = false;
function theme() {
    if (UI.rfb) UI.rfb.background = getComputedStyle(document.documentElement).getPropertyValue('--viewer-background').trim();
}
window.matchMedia('(prefers-color-scheme:dark)').addEventListener('change', theme);

UI.connect = async (event, password) => {
    if (launching || UI.rfb) return;
    launching = true;
    UI.hideStatus();
    UI.closeConnectPanel();
    UI.updateVisualState('connecting');
    try {
        const response = await fetch('/browser/start', {
            method: 'POST',
            headers: { 'X-Hermes-CSRF': '__CSRF__' },
            credentials: 'same-origin',
            signal: AbortSignal.timeout(65000),
        });
        if (!response.ok) throw new Error(await response.text());
        connect(event, password);
        theme();
    } catch (error) {
        UI.updateVisualState('disconnected');
        UI.openControlbar();
        UI.openConnectPanel();
        UI.showStatus(error.message, 'error');
    } finally {
        launching = false;
    }
};

await UI.start({
    settings: {
        defaults: { resize: 'scale' },
        mandatory: {
            host: '',
            port: 0,
            path: '/browser/websockify',
            encrypt: location.protocol === 'https:',
            shared: true,
            autoconnect: false,
            reconnect: false,
        },
    },
});
