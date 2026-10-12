package com.thomas.mobileagent;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.concurrent.TimeUnit;

import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.Response;
import okhttp3.WebSocket;
import okhttp3.WebSocketListener;

public class ThomasConnectionService extends Service {
    private static final int NOTIFICATION_ID = 4712;
    private static final String CHANNEL_ID = "thomas_mobile_connection";
    private static ThomasConnectionService instance;

    private final Handler main = new Handler(Looper.getMainLooper());
    private OkHttpClient client;
    private WebSocket socket;
    private int reconnectMs = 1000;
    private int serverIndex = 0;
    private boolean stopping;

    public static void start(Context context) {
        Intent intent = new Intent(context, ThomasConnectionService.class);
        if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(intent);
        else context.startService(intent);
    }

    public static void bindAccessibilityIfReady() {
        ThomasConnectionService current = instance;
        if (current == null) return;
        current.main.post(() -> {
            WebSocket ws = current.socket;
            if (ws != null) ThomasAccessibilityService.attachConnectionSocket(ws);
        });
    }

    public static void requestReconnect() {
        ThomasConnectionService current = instance;
        if (current == null) return;
        current.main.post(() -> {
            current.closeSocket();
            current.connect();
        });
    }

    @Override public void onCreate() {
        super.onCreate();
        instance = this;
        stopping = false;
        createChannel();
        try {
            Notification notification = buildNotification();
            if (Build.VERSION.SDK_INT >= 29) {
                startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
                );
            } else {
                startForeground(NOTIFICATION_ID, notification);
            }
        } catch (Throwable ignored) {}
        ThomasTls.Bundle tls = null;
        try { tls = ThomasTls.create(this); } catch (Throwable ignored) {}
        OkHttpClient.Builder clientBuilder = new OkHttpClient.Builder()
            .pingInterval(20, TimeUnit.SECONDS)
            .connectTimeout(8, TimeUnit.SECONDS)
            .readTimeout(0, TimeUnit.MILLISECONDS);
        if (tls != null) clientBuilder.sslSocketFactory(tls.socketFactory, tls.trustManager);
        client = clientBuilder.build();
        connect();
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (socket == null) connect();
        return START_STICKY;
    }

    private void connect() {
        if (stopping || socket != null) return;

        String server = prefs("server", "");
        String token = prefs("token", "");
        if (server.isEmpty() || token.isEmpty()) {
            scheduleReconnect();
            return;
        }

        java.util.List<String> servers = serverCandidates(server);
        if (servers.isEmpty()) {
            scheduleReconnect();
            return;
        }
        if (serverIndex >= servers.size()) serverIndex = 0;
        String targetServer = servers.get(serverIndex);

        Request request;
        try {
            request = new Request.Builder().url(targetServer).build();
        } catch (Throwable e) {
            advanceServer(servers.size());
            scheduleReconnect();
            return;
        }

        socket = client.newWebSocket(request, new ThomasWebSocketListener(this));
    }

    void onSocketOpen(WebSocket ws) {
        if (socket != ws) {
            try { ws.close(1000, "stale socket"); } catch (Throwable ignored) {}
            return;
        }
        reconnectMs = 1000;
        serverIndex = 0;
        ThomasAccessibilityService.attachConnectionSocket(ws);
        sendRegister(ws);
    }

    void onSocketMessage(WebSocket ws, String text) {
        ThomasAccessibilityService.attachConnectionSocket(ws);
        ThomasAccessibilityService.dispatchRemoteMessage(text);
    }

    void onSocketClosed(WebSocket ws) {
        ThomasAccessibilityService.detachConnectionSocket(ws);
        if (socket == ws) socket = null;
        scheduleReconnect();
    }

    void onSocketFailure(WebSocket ws) {
        ThomasAccessibilityService.detachConnectionSocket(ws);
        if (socket == ws) socket = null;
        java.util.List<String> candidates = serverCandidates(prefs("server", ""));
        advanceServer(candidates.size());
        scheduleReconnect();
    }

    private void advanceServer(int serverCount) {
        if (serverCount <= 0) {
            serverIndex = 0;
            return;
        }
        serverIndex = (serverIndex + 1) % serverCount;
    }

    private java.util.List<String> serverCandidates(String configured) {
        java.util.LinkedHashSet<String> out = new java.util.LinkedHashSet<>();
        if (configured != null) {
            for (String item : configured.split("[,;\n]+")) addServerCandidate(out, item);
        }
        // No built-in relay address. Only use the endpoint the user configured.
        return new java.util.ArrayList<>(out);
    }

    private void addServerCandidate(java.util.Set<String> out, String value) {
        if (value == null) return;
        String s = value.trim();
        if (s.isEmpty()) return;
        if (s.startsWith("http://")) s = "ws://" + s.substring(7);
        else if (s.startsWith("https://")) s = "wss://" + s.substring(8);
        try {
            Uri uri = Uri.parse(s);
            String scheme = uri.getScheme();
            String host = uri.getHost();
            if (scheme == null || host == null) return;
            String rebuilt = s;
            if (uri.getPath() == null || uri.getPath().isEmpty()) {
                rebuilt = s.endsWith("/") ? s + "agent" : s + "/agent";
            }
            out.add(rebuilt);
        } catch (Throwable ignored) {}
    }

    private void sendRegister(WebSocket ws) {
        try {
            JSONObject o = new JSONObject();
            o.put("type", "register");
            o.put("token", prefs("token", ""));
            o.put("deviceId", deviceId());
            o.put("name", Build.MANUFACTURER + " " + Build.MODEL);
            o.put("version", "0.5.3");
            JSONArray caps = new JSONArray();
            for (String s : new String[]{
                "observe","tap","long_press","swipe","type","clear_text",
                "back","home","recents","notifications","quick_settings",
                "power_menu","lock","wake","unlock","apps","launch","launch_app",
                "open_url","open_settings","screenshot","scroll","tap_text",
                "tap_description","find_text","wait_for_text",
                "clipboard_set","clipboard_get","device_info","battery",
                "screen_state","brightness","volume","media"
            }) caps.put(s);
            o.put("capabilities", caps);
            ws.send(o.toString());
        } catch (Throwable e) {
            closeSocket();
        }
    }

    private static final String PREFS = "thomas_mobile";
    private static final String KEY_DEVICE = "device_id";

    private String prefs(String key, String fallback) {
        return getSharedPreferences(PREFS, MODE_PRIVATE)
            .getString(key, fallback);
    }

    private String deviceId() {
        String id = prefs(KEY_DEVICE, "");
        if (!id.isEmpty()) return id;
        return Settings.Secure.getString(
            getContentResolver(), Settings.Secure.ANDROID_ID);
    }

    private void closeSocket() {
        WebSocket ws = socket;
        socket = null;
        if (ws != null) {
            try { ws.close(1000, "reconnect"); }
            catch (Throwable ignored) {}
        }
    }

    private void scheduleReconnect() {
        if (stopping) return;
        int delay = reconnectMs;
        reconnectMs = Math.min(15000, Math.max(1000, reconnectMs * 2));
        main.removeCallbacks(connectRunnable);
        main.postDelayed(connectRunnable, delay);
    }

    private final Runnable connectRunnable = this::connect;

    private void createChannel() {
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                "Thomas Mobile",
                NotificationManager.IMPORTANCE_LOW
            );
            NotificationManager nm = getSystemService(NotificationManager.class);
            if (nm != null) nm.createNotificationChannel(channel);
        }
    }

    private Notification buildNotification() {
        Notification.Builder builder = Build.VERSION.SDK_INT >= 26
            ? new Notification.Builder(this, CHANNEL_ID)
            : new Notification.Builder(this);
        return builder
            .setContentTitle("Thomas Mobile")
            .setContentText("Conexão com o Thomas ativa")
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .build();
    }

    @Override public void onDestroy() {
        stopping = true;
        main.removeCallbacks(connectRunnable);
        closeSocket();
        if (client != null) client.dispatcher().executorService().shutdown();
        instance = null;
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
