package com.thomas.mobileagent;

import android.Manifest;
import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.PowerManager;
import android.provider.Settings;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

import okhttp3.MediaType;
import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.RequestBody;
import okhttp3.Response;

public class MainActivity extends Activity {
    static final String PREFS = "thomas_mobile";
    static final String KEY_SERVER = "server";
    static final String KEY_TOKEN = "token";
    static final String KEY_USER = "username";
    static final String KEY_DEVICE = "device_id";
    static final int REQ_CAPTURE = 4101;
    static final int REQ_RUNTIME = 4102;
    // Intentionally unset: configure a trusted WSS endpoint supplied by your administrator.
    static final String DEFAULT_SERVER = "";

    private EditText server;
    private EditText username;
    private EditText password;
    private TextView status;

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        setContentView(R.layout.activity_main);

        SharedPreferences prefs = getSharedPreferences(PREFS, MODE_PRIVATE);
        server = findViewById(R.id.server);
        username = findViewById(R.id.username);
        password = findViewById(R.id.password);
        status = findViewById(R.id.status);

        Button login = findViewById(R.id.login);
        Button permissions = findViewById(R.id.permissions);
        Button accessibility = findViewById(R.id.accessibility);
        Button overlay = findViewById(R.id.overlay);
        Button battery = findViewById(R.id.battery);
        Button capture = findViewById(R.id.capture);
        Button logout = findViewById(R.id.logout);

        String deviceId = prefs.getString(KEY_DEVICE, "");
        if (deviceId.isEmpty()) {
            deviceId = Build.MANUFACTURER + "-" + Build.MODEL + "-" +
                Settings.Secure.getString(getContentResolver(), Settings.Secure.ANDROID_ID);
            prefs.edit().putString(KEY_DEVICE, deviceId).apply();
        }

        String savedServer = prefs.getString(KEY_SERVER, "");
        if (isLegacyLocalEndpoint(savedServer)) {
            // Remove stale environment-specific URLs instead of substituting another host.
            savedServer = "";
            prefs.edit().putString(KEY_SERVER, "").apply();
        }
        server.setText(savedServer);
        username.setText(prefs.getString(KEY_USER, ""));
        status.setText("Thomas Mobile • pronto para login\nID: " + deviceId);

        final String finalDeviceId = deviceId;
        login.setOnClickListener(v -> loginAndConnect(finalDeviceId));
        permissions.setOnClickListener(v -> requestUsefulPermissions());
        accessibility.setOnClickListener(v ->
            startActivity(new Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
        );
        overlay.setOnClickListener(v -> {
            try {
                if (Build.VERSION.SDK_INT >= 23) {
                    Intent i = new Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION);
                    i.setData(Uri.parse("package:" + getPackageName()));
                    startActivity(i);
                } else {
                    startActivity(new Intent(Settings.ACTION_SETTINGS));
                }
            } catch (Throwable t) {
                startActivity(new Intent(Settings.ACTION_SETTINGS));
            }
        });
        battery.setOnClickListener(v -> {
            try {
                PowerManager pm = (PowerManager)getSystemService(POWER_SERVICE);
                if (Build.VERSION.SDK_INT >= 23 && pm != null &&
                    !pm.isIgnoringBatteryOptimizations(getPackageName())) {
                    Intent i = new Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS);
                    i.setData(Uri.parse("package:" + getPackageName()));
                    startActivity(i);
                } else {
                    startActivity(new Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS));
                }
            } catch (Throwable t) {
                startActivity(new Intent(Settings.ACTION_SETTINGS));
            }
        });
        capture.setOnClickListener(v -> requestCapturePermission());
        logout.setOnClickListener(v -> logout());

        if (!prefs.getString(KEY_TOKEN, "").isEmpty()) {
            ThomasConnectionService.start(this);
            status.setText("Sessão salva • conectando o Thomas Mobile…\nID: " + deviceId);

        }
    }

    private boolean isLegacyLocalEndpoint(String value) {
        String s = String.valueOf(value == null ? "" : value).trim().toLowerCase(java.util.Locale.ROOT);
        if (s.isEmpty()) return false;
        if (s.contains("tailscale") || s.contains("trycloudflare.com")) return true;
        String host;
        try {
            Uri uri = Uri.parse(s.contains("://") ? s : "wss://" + s);
            host = uri.getHost();
        } catch (Throwable ignored) {
            return false;
        }
        if (host == null) return false;
        return host.matches("^(?:10\\..*|192\\.168\\..*|172\\.(?:1[6-9]|2[0-9]|3[01])\\..*|100\\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\\..*)$");
    }

    private String httpsBase() {
        String s = server.getText().toString().trim();
        if (s.startsWith("wss://")) s = "https://" + s.substring(6);
        else if (s.startsWith("ws://")) s = "http://" + s.substring(5);
        else if (!s.startsWith("https://") && !s.startsWith("http://")) s = "https://" + s;
        int agentPath = s.indexOf("/agent");
        if (agentPath >= 0) s = s.substring(0, agentPath);
        return s;
    }

    private void loginAndConnect(String deviceId) {
        final String configuredEndpoint = server.getText().toString().trim();
        final Uri endpointUri = Uri.parse(configuredEndpoint);
        if (!"wss".equalsIgnoreCase(endpointUri.getScheme()) ||
            endpointUri.getHost() == null || endpointUri.getUserInfo() != null) {
            status.setText("Informe um endpoint WSS válido e confiável antes de entrar.");
            return;
        }
        final String base = httpsBase();
        final String user = username.getText().toString().trim();
        final String pass = password.getText().toString();
        if (user.isEmpty() || pass.isEmpty()) {
            status.setText("Informe usuário e senha.");
            return;
        }

        status.setText("Entrando no Thomas…");
        new Thread(() -> {
            try {
                ThomasTls.Bundle tls = ThomasTls.create(this);
                OkHttpClient client = new OkHttpClient.Builder()
                    .sslSocketFactory(tls.socketFactory, tls.trustManager)
                    .connectTimeout(10, TimeUnit.SECONDS)
                    .readTimeout(15, TimeUnit.SECONDS)
                    .build();

                JSONObject body = new JSONObject();
                body.put("username", user);
                body.put("password", pass);

                Request req = new Request.Builder()
                    .url(base + "/login")
                    .post(RequestBody.create(
                        body.toString(),
                        MediaType.get("application/json; charset=utf-8")
                    ))
                    .build();

                try (Response response = client.newCall(req).execute()) {
                    String raw = response.body() == null ? "{}" : response.body().string();
                    JSONObject data = new JSONObject(raw);
                    if (!response.isSuccessful() || !data.optBoolean("ok", false)) {
                        throw new Exception(data.optString("error", "Login recusado"));
                    }

                    String token = data.optString("sessionToken", "");
                    if (token.isEmpty()) throw new Exception("Servidor não devolveu sessão.");

                    getSharedPreferences(PREFS, MODE_PRIVATE).edit()
                        .putString(KEY_SERVER, server.getText().toString().trim())
                        .putString(KEY_USER, user)
                        .putString(KEY_TOKEN, token)
                        .putString(KEY_DEVICE, deviceId)
                        .apply();

                    runOnUiThread(() -> {
                        password.setText("");
                        status.setText("Login confirmado • Thomas conectado.\nUsuário: " + user);
                        ThomasConnectionService.start(MainActivity.this);
                        ThomasConnectionService.requestReconnect();

                    });
                }
            } catch (Throwable e) {
                runOnUiThread(() -> status.setText("Falha no login: " + e.getMessage()));
            }
        }).start();
    }

    private void logout() {
        SharedPreferences p = getSharedPreferences(PREFS, MODE_PRIVATE);
        String token = p.getString(KEY_TOKEN, "");
        p.edit().remove(KEY_TOKEN).apply();
        stopService(new Intent(this, ThomasConnectionService.class));
        status.setText("Sessão encerrada.");
        if (!token.isEmpty()) {
            new Thread(() -> {
                try {
                    ThomasTls.Bundle tls = ThomasTls.create(this);
                    OkHttpClient client = new OkHttpClient.Builder()
                        .sslSocketFactory(tls.socketFactory, tls.trustManager)
                        .connectTimeout(8, TimeUnit.SECONDS).readTimeout(8, TimeUnit.SECONDS).build();
                    Request req = new Request.Builder().url(httpsBase() + "/logout")
                        .addHeader("Authorization", "Bearer " + token).post(
                            RequestBody.create(new byte[0], MediaType.get("application/json"))
                        ).build();
                    client.newCall(req).execute().close();
                } catch (Throwable ignored) {}
            }).start();
        }
    }

    private void requestUsefulPermissions() {
        List<String> list = new ArrayList<>();
        addPermission(list, Manifest.permission.RECORD_AUDIO);
        addPermission(list, Manifest.permission.CAMERA);
        if (Build.VERSION.SDK_INT >= 33) {
            addPermission(list, Manifest.permission.POST_NOTIFICATIONS);
            addPermission(list, Manifest.permission.NEARBY_WIFI_DEVICES);
            addPermission(list, Manifest.permission.READ_MEDIA_IMAGES);
            addPermission(list, Manifest.permission.READ_MEDIA_VIDEO);
            addPermission(list, Manifest.permission.READ_MEDIA_AUDIO);
        } else {
            addPermission(list, Manifest.permission.READ_EXTERNAL_STORAGE);
            addPermission(list, Manifest.permission.WRITE_EXTERNAL_STORAGE);
        }
        if (Build.VERSION.SDK_INT >= 31) {
            addPermission(list, Manifest.permission.BLUETOOTH_SCAN);
            addPermission(list, Manifest.permission.BLUETOOTH_CONNECT);
            addPermission(list, Manifest.permission.BLUETOOTH_ADVERTISE);
        } else {
            addPermission(list, Manifest.permission.ACCESS_FINE_LOCATION);
            addPermission(list, Manifest.permission.ACCESS_COARSE_LOCATION);
        }
        if (Build.VERSION.SDK_INT >= 23 && !list.isEmpty()) {
            requestPermissions(list.toArray(new String[0]), REQ_RUNTIME);
        }
    }

    private void addPermission(List<String> list, String p) {
        if (Build.VERSION.SDK_INT >= 23 && checkSelfPermission(p) != PackageManager.PERMISSION_GRANTED) {
            list.add(p);
        }
    }

    private void requestCapturePermission() {
        android.media.projection.MediaProjectionManager manager =
            (android.media.projection.MediaProjectionManager)getSystemService(MEDIA_PROJECTION_SERVICE);
        if (manager == null) return;
        startActivityForResult(manager.createScreenCaptureIntent(), REQ_CAPTURE);
    }

    @Override protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQ_CAPTURE || data == null) return;
        ScreenCaptureService.startWithPermission(this, resultCode, data);
        if (status != null) status.setText("Captura da tela autorizada.");
    }
}
