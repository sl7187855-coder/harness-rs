package com.thomas.mobileagent;

import okhttp3.Response;
import okhttp3.WebSocket;
import okhttp3.WebSocketListener;

final class ThomasWebSocketListener extends WebSocketListener {
    private final ThomasConnectionService service;

    ThomasWebSocketListener(ThomasConnectionService service) {
        this.service = service;
    }

    @Override public void onOpen(WebSocket ws, Response response) {
        service.onSocketOpen(ws);
    }

    @Override public void onMessage(WebSocket ws, String text) {
        service.onSocketMessage(ws, text);
    }

    @Override public void onClosed(WebSocket ws, int code, String reason) {
        service.onSocketClosed(ws);
    }

    @Override public void onFailure(WebSocket ws, Throwable t, Response response) {
        service.onSocketFailure(ws);
    }
}
