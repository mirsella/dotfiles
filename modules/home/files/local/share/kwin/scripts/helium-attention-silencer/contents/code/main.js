const TARGET_RESOURCE_CLASS = "helium";
const ATTENTION_DELAY_MS = 2000;

function clearAttention(window) {
    if (window.resourceClass === TARGET_RESOURCE_CLASS && window.demandsAttention) {
        window.demandsAttention = false;
    }
}

function watchWindow(window) {
    if (window.resourceClass !== TARGET_RESOURCE_CLASS) {
        return;
    }

    const timer = new QTimer(window);
    timer.interval = ATTENTION_DELAY_MS;
    timer.singleShot = true;
    timer.timeout.connect(function () {
        clearAttention(window);
    });

    window.demandsAttentionChanged.connect(function () {
        if (window.demandsAttention) {
            timer.start();
        } else {
            timer.stop();
        }
    });

    if (window.demandsAttention) {
        timer.start();
    }
}

workspace.windowList().forEach(watchWindow);
workspace.windowAdded.connect(watchWindow);
