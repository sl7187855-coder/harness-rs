package com.thomas.mobileagent;

import android.accessibilityservice.AccessibilityService;
import android.accessibilityservice.GestureDescription;
import android.graphics.Bitmap;
import android.graphics.ColorSpace;
import android.graphics.Path;
import android.graphics.Rect;
import android.hardware.HardwareBuffer;
import android.os.Bundle;
import android.os.Build;
import android.os.BatteryManager;
import android.os.PowerManager;
import android.os.SystemClock;
import android.app.KeyguardManager;
import android.media.AudioManager;
import android.provider.Settings;
import android.os.Handler;
import android.os.Looper;

import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;
import android.content.Intent;
import android.content.pm.ResolveInfo;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.TimeUnit;

import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.Response;
import okhttp3.WebSocket;
import okhttp3.WebSocketListener;

public class ThomasAccessibilityService extends AccessibilityService {
    private static ThomasAccessibilityService instance;
    private final Handler main = new Handler(Looper.getMainLooper());
    private WebSocket socket;

    public static void requestReconnect() {
        ThomasConnectionService.requestReconnect();
    }

    public static void attachConnectionSocket(WebSocket ws) {
        if (instance != null) instance.socket = ws;
    }

    public static void detachConnectionSocket(WebSocket ws) {
        if (instance != null && instance.socket == ws) instance.socket = null;
    }

    public static void dispatchRemoteMessage(String raw) {
        ThomasAccessibilityService target = instance;
        if (target == null) return;
        target.main.post(() -> target.handleMessage(raw));
    }

    @Override
    protected void onServiceConnected() {
        super.onServiceConnected();
        instance = this;
        ThomasConnectionService.bindAccessibilityIfReady();
        WebSocket current = socket;
        if (current != null) {
            // Connection is owned by ThomasConnectionService.
        }
    }

    private void handleMessage(String raw) {
        try {
            JSONObject msg = new JSONObject(raw);
            if (!"command".equals(msg.optString("type"))) return;
            String requestId = msg.optString(
                "requestId", UUID.randomUUID().toString());
            String action = msg.optString("action", "");
            JSONObject p = msg.optJSONObject("params");
            if (p == null) p = new JSONObject();

            switch (action) {
                case "observe": sendResult(requestId, observe(), null); break;
                case "tap": tap(requestId, p); break;
                case "long_press": longPress(requestId, p); break;
                case "swipe": swipe(requestId, p); break;
                case "type": typeText(requestId, p.optString("text", "")); break;
                case "clear_text": clearFocusedText(requestId); break;
                case "scroll": scroll(requestId, p); break;
                case "tap_text": p.put("field", "text"); tapNode(requestId, p); break;
                case "tap_description": p.put("field", "description"); tapNode(requestId, p); break;
                case "find_text": findElements(requestId, p); break;
                case "wait_for_text": waitForText(requestId, p); break;
                case "clipboard_set": clipboardSet(requestId, p.optString("text", "")); break;
                case "clipboard_get": clipboardGet(requestId); break;
                case "open_url": openUrl(requestId, p.optString("url", "")); break;
                case "back": globalAction(requestId, GLOBAL_ACTION_BACK); break;
                case "home": globalAction(requestId, GLOBAL_ACTION_HOME); break;
                case "recents": globalAction(requestId, GLOBAL_ACTION_RECENTS); break;
                case "notifications": globalAction(requestId, GLOBAL_ACTION_NOTIFICATIONS); break;
                case "quick_settings": globalAction(requestId, GLOBAL_ACTION_QUICK_SETTINGS); break;
                case "power_menu": globalAction(requestId, GLOBAL_ACTION_POWER_DIALOG); break;
                case "lock": globalAction(requestId, GLOBAL_ACTION_LOCK_SCREEN); break;
                case "wake": wakeScreen(requestId); break;
                case "unlock": unlockScreen(requestId); break;
                case "open_settings": openSettings(requestId, p); break;
                case "brightness": setBrightness(requestId, p); break;
                case "volume": setVolume(requestId, p); break;
                case "screen_state": screenState(requestId); break;
                case "battery": batteryState(requestId); break;
                case "device_info": deviceInfo(requestId); break;
                case "reboot": rebootDevice(requestId); break;
                case "apps": listLaunchableApps(requestId); break;
                case "launch": launch(requestId, p.optString("packageName", "")); break;
                case "launch_app": launchByLabel(requestId, p.optString("label", "")); break;
                case "screenshot": screenshot(requestId); break;
                default: sendResult(requestId, null, "unknown action: " + action);
            }
        } catch (Exception e) {
            sendErrorFromRaw(raw, e);
        }
    }
    private void tap(String requestId, JSONObject p) {
        float x = (float) p.optDouble("x", 0);
        float y = (float) p.optDouble("y", 0);
        Path path = new Path();
        path.moveTo(x, y);
        GestureDescription.StrokeDescription stroke =
            new GestureDescription.StrokeDescription(path, 0, 50);
        dispatchGesture(
            new GestureDescription.Builder().addStroke(stroke).build(),
            simpleGestureCallback(requestId), null);
    }

    private void swipe(String requestId, JSONObject p) {
        float x1 = (float) p.optDouble("x1", 0);
        float y1 = (float) p.optDouble("y1", 0);
        float x2 = (float) p.optDouble("x2", 0);
        float y2 = (float) p.optDouble("y2", 0);
        long duration = Math.max(80, Math.min(1500,
            p.optLong("durationMs", 350)));
        Path path = new Path();
        path.moveTo(x1, y1);
        path.lineTo(x2, y2);
        GestureDescription.StrokeDescription stroke =
            new GestureDescription.StrokeDescription(path, 0, duration);
        dispatchGesture(
            new GestureDescription.Builder().addStroke(stroke).build(),
            simpleGestureCallback(requestId), null);
    }

    private AccessibilityService.GestureResultCallback simpleGestureCallback(
        String requestId) {
        return new AccessibilityService.GestureResultCallback() {
            @Override public void onCompleted(
                GestureDescription gestureDescription) {
                sendResult(requestId, new JSONObject(), null);
            }

            @Override public void onCancelled(
                GestureDescription gestureDescription) {
                sendResult(requestId, null, "gesture cancelled");
            }
        };
    }
    private void typeText(String requestId, String text) {
        AccessibilityNodeInfo root = getRootInActiveWindow();
        AccessibilityNodeInfo focus = root == null ? null :
            root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT);
        if (focus == null && root != null) {
            focus = root.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY);
        }
        if (focus == null || !focus.isEditable()) {
            sendResult(requestId, null, "no focused editable field");
            if (root != null) root.recycle();
            return;
        }
        Bundle args = new Bundle();
        args.putCharSequence(
            AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text);
        boolean ok = focus.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT, args);
        focus.recycle();
        if (root != null) root.recycle();
        if (ok) sendResult(requestId, new JSONObject(), null);
        else sendResult(requestId, null, "text input rejected");
    }

    private void globalAction(String requestId, int action) {
        sendResult(requestId, new JSONObject(),
            performGlobalAction(action) ? null : "global action rejected");
    }

    private void listLaunchableApps(String requestId) {
        try {
            Intent main = new Intent(Intent.ACTION_MAIN);
            main.addCategory(Intent.CATEGORY_LAUNCHER);
            List<ResolveInfo> infos = getPackageManager().queryIntentActivities(main, 0);
            Collections.sort(infos, Comparator.comparing(
                x -> String.valueOf(x.loadLabel(getPackageManager())),
                String.CASE_INSENSITIVE_ORDER));
            JSONArray apps = new JSONArray();
            for (ResolveInfo info : infos) {
                if (info.activityInfo == null) continue;
                String pkg = info.activityInfo.packageName;
                String label = String.valueOf(info.loadLabel(getPackageManager()));
                if (pkg.isEmpty()) continue;
                JSONObject app = new JSONObject();
                app.put("label", label);
                app.put("packageName", pkg);
                apps.put(app);
            }
            JSONObject result = new JSONObject();
            result.put("apps", apps);
            result.put("count", apps.length());
            sendResult(requestId, result, null);
        } catch (Exception e) {
            sendResult(requestId, null, "apps failed: " + e.getMessage());
        }
    }

    private void launchByLabel(String requestId, String label) {
        try {
            String query = label == null ? "" : label.trim();
            if (query.isEmpty()) {
                sendResult(requestId, null, "label required");
                return;
            }
            Intent main = new Intent(Intent.ACTION_MAIN);
            main.addCategory(Intent.CATEGORY_LAUNCHER);
            List<ResolveInfo> infos = getPackageManager().queryIntentActivities(main, 0);
            String bestPkg = null;
            String bestLabel = null;
            for (ResolveInfo info : infos) {
                if (info.activityInfo == null) continue;
                String candidate = String.valueOf(info.loadLabel(getPackageManager()));
                if (candidate.equalsIgnoreCase(query)) {
                    bestPkg = info.activityInfo.packageName;
                    bestLabel = candidate;
                    break;
                }
                if (bestPkg == null && candidate.toLowerCase().contains(query.toLowerCase())) {
                    bestPkg = info.activityInfo.packageName;
                    bestLabel = candidate;
                }
            }
            if (bestPkg == null) {
                sendResult(requestId, null, "app not found: " + query);
                return;
            }
            Intent launch = getPackageManager().getLaunchIntentForPackage(bestPkg);
            if (launch == null) {
                sendResult(requestId, null, "launch intent not found: " + bestPkg);
                return;
            }
            launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(launch);
            JSONObject result = new JSONObject();
            result.put("packageName", bestPkg);
            result.put("label", bestLabel);
            sendResult(requestId, result, null);
        } catch (Exception e) {
            sendResult(requestId, null, "launch app failed: " + e.getMessage());
        }
    }

    private void launch(String requestId, String pkg) {
        try {
            if (pkg.isEmpty()) {
                sendResult(requestId, null, "packageName required");
                return;
            }
            Intent launch = getPackageManager().getLaunchIntentForPackage(pkg);
            if (launch == null) {
                sendResult(requestId, null, "package not found: " + pkg);
                return;
            }
            launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(launch);
            sendResult(requestId, new JSONObject(), null);
        } catch (Exception e) {
            sendResult(requestId, null, "launch failed: " + e.getMessage());
        }
    }

    private void screenshot(String requestId) {
        if (Build.VERSION.SDK_INT >= 30) {
            try {
                takeScreenshot(
                    android.view.Display.DEFAULT_DISPLAY,
                    getMainExecutor(),
                    new TakeScreenshotCallback() {
                        @Override public void onSuccess(ScreenshotResult shot) {
                            HardwareBuffer buffer = null;
                            Bitmap hardware = null;
                            Bitmap bitmap = null;
                            try {
                                buffer = shot.getHardwareBuffer();
                                if (buffer == null) {
                                    sendResult(requestId, null, "accessibility screenshot returned no buffer");
                                    return;
                                }
                                hardware = Bitmap.wrapHardwareBuffer(buffer, shot.getColorSpace());
                                if (hardware == null) {
                                    sendResult(requestId, null, "accessibility screenshot bitmap unavailable");
                                    return;
                                }
                                bitmap = hardware.copy(Bitmap.Config.ARGB_8888, false);
                                if (bitmap == null) {
                                    sendResult(requestId, null, "accessibility screenshot conversion failed");
                                    return;
                                }
                                ByteArrayOutputStream out = new ByteArrayOutputStream();
                                if (!bitmap.compress(Bitmap.CompressFormat.JPEG, 82, out)) {
                                    sendResult(requestId, null, "accessibility screenshot JPEG failed");
                                    return;
                                }
                                JSONObject result = new JSONObject();
                                result.put("mime", "image/jpeg");
                                result.put("data", android.util.Base64.encodeToString(
                                    out.toByteArray(), android.util.Base64.NO_WRAP));
                                sendResult(requestId, result, null);
                            } catch (Throwable e) {
                                sendResult(requestId, null, "accessibility screenshot failed: " + e.getMessage());
                            } finally {
                                if (bitmap != null && !bitmap.isRecycled()) bitmap.recycle();
                                if (hardware != null && !hardware.isRecycled()) hardware.recycle();
                                if (buffer != null) {
                                    try { buffer.close(); } catch (Throwable ignored) {}
                                }
                            }
                        }

                        @Override public void onFailure(int errorCode) {
                            sendResult(requestId, null,
                                "accessibility screenshot failed: " + errorCode);
                        }
                    });
                return;
            } catch (Throwable ignored) {
                // Fall back to the existing MediaProjection capture service.
            }
        }

        ScreenCaptureService.captureJpeg(new ScreenCaptureService.Callback() {
            @Override public void onSuccess(byte[] jpeg) {
                try {
                    JSONObject result = new JSONObject();
                    result.put("mime", "image/jpeg");
                    result.put("data", android.util.Base64.encodeToString(
                        jpeg, android.util.Base64.NO_WRAP));
                    sendResult(requestId, result, null);
                } catch (Exception e) {
                    sendResult(requestId, null, "screenshot encoding failed");
                }
            }

            @Override public void onError(String error) {
                sendResult(requestId, null, error);
            }
        });
    }
    private JSONObject observe() throws Exception {
        JSONObject out = new JSONObject();
        android.util.DisplayMetrics dm = getResources().getDisplayMetrics();
        out.put("width", dm.widthPixels);
        out.put("height", dm.heightPixels);

        AccessibilityNodeInfo root = getRootInActiveWindow();
        out.put("packageName", root == null ? "" :
            String.valueOf(root.getPackageName()));

        JSONArray elements = new JSONArray();
        if (root != null) {
            int[] count = {0};
            walk(root, elements, count);
            root.recycle();
        }
        out.put("elements", elements);
        return out;
    }

    private void walk(AccessibilityNodeInfo node, JSONArray out, int[] count) {
        if (node == null || count[0] >= 350) return;
        try {
            JSONObject e = new JSONObject();
            String text = node.getText() == null ? "" :
                node.getText().toString();
            String desc = node.getContentDescription() == null ? "" :
                node.getContentDescription().toString();
            String viewId = node.getViewIdResourceName() == null ? "" :
                node.getViewIdResourceName();
            Rect r = new Rect();
            node.getBoundsInScreen(r);

            e.put("text", text);
            e.put("description", desc);
            e.put("resourceId", viewId);
            e.put("className", String.valueOf(node.getClassName()));
            e.put("clickable", node.isClickable());
            e.put("editable", node.isEditable());
            e.put("enabled", node.isEnabled());
            e.put("x", r.left);
            e.put("y", r.top);
            e.put("width", r.width());
            e.put("height", r.height());

            if (!text.isEmpty() || !desc.isEmpty() ||
                node.isClickable() || node.isEditable()) {
                out.put(e);
                count[0]++;
            }

            for (int i = 0;
                 i < node.getChildCount() && count[0] < 350; i++) {
                AccessibilityNodeInfo child = node.getChild(i);
                if (child != null) {
                    walk(child, out, count);
                    child.recycle();
                }
            }
        } catch (Throwable ignored) {
        }
    }
    private void wakeScreen(String requestId) {
        try {
            PowerManager pm = (PowerManager) getSystemService(POWER_SERVICE);
            if (pm == null) throw new Exception("power manager unavailable");
            PowerManager.WakeLock wl = pm.newWakeLock(
                PowerManager.SCREEN_BRIGHT_WAKE_LOCK | PowerManager.ACQUIRE_CAUSES_WAKEUP,
                "ThomasMobile:Wake");
            wl.acquire(1500);
            if (wl.isHeld()) wl.release();
            sendResult(requestId, new JSONObject(), null);
        } catch (Exception e) { sendResult(requestId, null, "wake failed: " + e.getMessage()); }
    }

    private void unlockScreen(String requestId) {
        try {
            KeyguardManager km = (KeyguardManager) getSystemService(KEYGUARD_SERVICE);
            if (km == null) throw new Exception("keyguard unavailable");
            boolean locked = km.isKeyguardLocked();
            if (!locked) { sendResult(requestId, new JSONObject(), null); return; }
            Intent intent = km.createConfirmDeviceCredentialIntent(
                "Desbloquear aparelho", "Autentique o aparelho para o Thomas continuar.");
            if (intent == null) throw new Exception("o sistema não permite iniciar autenticação");
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(intent);
            JSONObject out = new JSONObject(); out.put("authenticationRequired", true);
            sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "unlock failed: " + e.getMessage()); }
    }

    private void openSettings(String requestId, JSONObject p) {
        try {
            String kind = p.optString("kind", "settings").toLowerCase();
            String action = Settings.ACTION_SETTINGS;
            if ("wifi".equals(kind)) action = Build.VERSION.SDK_INT >= 29 ? Settings.Panel.ACTION_INTERNET_CONNECTIVITY : Settings.ACTION_WIFI_SETTINGS;
            else if ("bluetooth".equals(kind)) action = Settings.ACTION_BLUETOOTH_SETTINGS;
            else if ("accessibility".equals(kind)) action = Settings.ACTION_ACCESSIBILITY_SETTINGS;
            else if ("security".equals(kind)) action = Settings.ACTION_SECURITY_SETTINGS;
            Intent i = new Intent(action); i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK); startActivity(i);
            JSONObject out = new JSONObject(); out.put("action", action); sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "settings failed: " + e.getMessage()); }
    }

    private void setBrightness(String requestId, JSONObject p) {
        try {
            if (!Settings.System.canWrite(this)) {
                Intent i = new Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS,
                    android.net.Uri.parse("package:" + getPackageName()));
                i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK); startActivity(i);
                sendResult(requestId, null, "WRITE_SETTINGS permission required"); return;
            }
            int value = p.has("value") ? p.optInt("value", -1) : -1;
            if (value < 0 && p.has("percent")) value = Math.round(Math.max(0, Math.min(100, (float)p.optDouble("percent", 50))) * 255f / 100f);
            if (value < 0) { value = Settings.System.getInt(getContentResolver(), Settings.System.SCREEN_BRIGHTNESS, 128); }
            value = Math.max(0, Math.min(255, value));
            Settings.System.putInt(getContentResolver(), Settings.System.SCREEN_BRIGHTNESS, value);
            JSONObject out = new JSONObject(); out.put("value", value); out.put("percent", Math.round(value * 100f / 255f)); sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "brightness failed: " + e.getMessage()); }
    }

    private void setVolume(String requestId, JSONObject p) {
        try {
            AudioManager am = (AudioManager)getSystemService(AUDIO_SERVICE);
            if (am == null) throw new Exception("audio manager unavailable");
            String dir = p.optString("direction", "").toLowerCase();
            int stream = p.optInt("stream", AudioManager.STREAM_MUSIC);
            if (p.has("value")) {
                int max = am.getStreamMaxVolume(stream), value = Math.max(0, Math.min(max, p.optInt("value", am.getStreamVolume(stream))));
                am.setStreamVolume(stream, value, 0);
            } else if ("up".equals(dir)) am.adjustStreamVolume(stream, AudioManager.ADJUST_RAISE, 0);
            else if ("down".equals(dir)) am.adjustStreamVolume(stream, AudioManager.ADJUST_LOWER, 0);
            else if ("mute".equals(dir)) am.adjustStreamVolume(stream, AudioManager.ADJUST_MUTE, 0);
            else if ("unmute".equals(dir)) am.adjustStreamVolume(stream, AudioManager.ADJUST_UNMUTE, 0);
            JSONObject out = new JSONObject(); out.put("value", am.getStreamVolume(stream)); out.put("max", am.getStreamMaxVolume(stream)); sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "volume failed: " + e.getMessage()); }
    }

    private void screenState(String requestId) {
        try {
            PowerManager pm = (PowerManager)getSystemService(POWER_SERVICE); KeyguardManager km = (KeyguardManager)getSystemService(KEYGUARD_SERVICE);
            JSONObject out = new JSONObject(); out.put("interactive", pm != null && pm.isInteractive()); out.put("locked", km != null && km.isKeyguardLocked()); out.put("secure", km != null && km.isKeyguardSecure()); sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "screen state failed: " + e.getMessage()); }
    }

    private void batteryState(String requestId) {
        try {
            BatteryManager bm = (BatteryManager)getSystemService(BATTERY_SERVICE);
            JSONObject out = new JSONObject();
            out.put("percent", bm == null ? -1 : bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY));
            out.put("charging", bm != null && bm.isCharging());
            out.put("temperatureSource", "system");
            sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "battery failed: " + e.getMessage()); }
    }

    private void deviceInfo(String requestId) {
        try {
            android.util.DisplayMetrics dm = getResources().getDisplayMetrics();
            JSONObject out = new JSONObject();
            out.put("manufacturer", android.os.Build.MANUFACTURER); out.put("model", android.os.Build.MODEL); out.put("brand", android.os.Build.BRAND);
            out.put("android", android.os.Build.VERSION.RELEASE); out.put("sdk", android.os.Build.VERSION.SDK_INT); out.put("width", dm.widthPixels); out.put("height", dm.heightPixels);
            out.put("locale", java.util.Locale.getDefault().toLanguageTag()); sendResult(requestId, out, null);
        } catch (Exception e) { sendResult(requestId, null, "device info failed: " + e.getMessage()); }
    }

    private void rebootDevice(String requestId) {
        try {
            android.app.admin.DevicePolicyManager dpm = (android.app.admin.DevicePolicyManager)getSystemService(DEVICE_POLICY_SERVICE);
            if (dpm == null || !dpm.isDeviceOwnerApp(getPackageName())) { sendResult(requestId, null, "reboot requires Thomas Mobile to be Device Owner"); return; }
            dpm.reboot(null);
            sendResult(requestId, new JSONObject(), null);
        } catch (Exception e) { sendResult(requestId, null, "reboot failed: " + e.getMessage()); }
    }

    private void sendResult(String requestId, JSONObject result, String error) {
        try {
            JSONObject msg = new JSONObject();
            msg.put("type", "result");
            msg.put("requestId", requestId);
            msg.put("ok", error == null);
            if (error != null) msg.put("error", error);
            else msg.put("result",
                result == null ? new JSONObject() : result);
            sendRaw(msg.toString());
        } catch (Exception ignored) {
        }
    }

    private void sendErrorFromRaw(String raw, Exception error) {
        try {
            JSONObject msg = new JSONObject(raw);
            sendResult(msg.optString("requestId", ""), null,
                "command error: " + error.getMessage());
        } catch (Exception ignored) {
        }
    }

    private void sendRaw(String text) {
        WebSocket ws = socket;
        if (ws != null) {
            ws.send(text);
        }
    }

    @Override public void onAccessibilityEvent(AccessibilityEvent event) {}

    @Override public void onInterrupt() {}

    @Override public boolean onUnbind(Intent intent) {
        instance = null;
        socket = null;
        return super.onUnbind(intent);
    }

    @Override public void onDestroy() {
        instance = null;
        socket = null;
        super.onDestroy();
    }
    private void longPress(String requestId, JSONObject p) {
        float x = (float) p.optDouble("x", 0);
        float y = (float) p.optDouble("y", 0);
        long duration = Math.max(500, Math.min(1800, p.optLong("durationMs", 700)));
        Path path = new Path();
        path.moveTo(x, y);
        GestureDescription.StrokeDescription stroke =
            new GestureDescription.StrokeDescription(path, 0, duration);
        dispatchGesture(new GestureDescription.Builder().addStroke(stroke).build(),
            simpleGestureCallback(requestId), null);
    }

    private void openUrl(String requestId, String url) {
        try {
            if (url == null || url.trim().isEmpty()) {
                sendResult(requestId, null, "url required");
                return;
            }
            Intent intent = new Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url.trim()));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(intent);
            sendResult(requestId, new JSONObject(), null);
        } catch (Exception e) {
            sendResult(requestId, null, "open url failed: " + e.getMessage());
        }
    }

    private AccessibilityNodeInfo findNode(String field, String needle) {
        AccessibilityNodeInfo root = getRootInActiveWindow();
        if (root == null) return null;
        AccessibilityNodeInfo found = findNodeRecursive(root, field, needle.toLowerCase());
        if (found != null) return found;
        root.recycle();
        return null;
    }

    private AccessibilityNodeInfo findNodeRecursive(
        AccessibilityNodeInfo node, String field, String needle) {
        String value = "";
        if ("text".equals(field) && node.getText() != null) value = node.getText().toString();
        else if ("description".equals(field) && node.getContentDescription() != null) value = node.getContentDescription().toString();
        else if ("resourceId".equals(field) && node.getViewIdResourceName() != null) value = node.getViewIdResourceName();
        if (!value.isEmpty() && value.toLowerCase().contains(needle)) return node;
        for (int i = 0; i < node.getChildCount(); i++) {
            AccessibilityNodeInfo child = node.getChild(i);
            if (child == null) continue;
            AccessibilityNodeInfo found = findNodeRecursive(child, field, needle);
            if (found != null) {
                child.recycle();
                return found;
            }
            child.recycle();
        }
        return null;
    }

    private void tapNode(String requestId, JSONObject p) {
        String field = p.optString("field", "text");
        String query = p.optString("query", "");
        AccessibilityNodeInfo node = findNode(field, query);
        if (node == null) {
            sendResult(requestId, null, "element not found: " + field + "=" + query);
            return;
        }
        boolean clicked = node.isEnabled() && node.isClickable() &&
            node.performAction(AccessibilityNodeInfo.ACTION_CLICK);
        Rect r = new Rect();
        node.getBoundsInScreen(r);
        node.recycle();
        if (clicked) {
            sendResult(requestId, new JSONObject(), null);
            return;
        }
        float cx = r.centerX();
        float cy = r.centerY();
        Path path = new Path();
        path.moveTo(cx, cy);
        GestureDescription.StrokeDescription stroke =
            new GestureDescription.StrokeDescription(path, 0, 60);
        dispatchGesture(
            new GestureDescription.Builder().addStroke(stroke).build(),
            simpleGestureCallback(requestId), null);
    }

    private void findElements(String requestId, JSONObject p) {
        String field = p.optString("field", "text");
        String query = p.optString("query", "").toLowerCase();
        try {
            JSONArray matches = new JSONArray();
            AccessibilityNodeInfo root = getRootInActiveWindow();
            if (root != null) {
                collectMatches(root, field, query, matches, new int[]{0});
                root.recycle();
            }
            JSONObject out = new JSONObject();
            out.put("matches", matches);
            out.put("count", matches.length());
            sendResult(requestId, out, null);
        } catch (Exception e) {
            sendResult(requestId, null, "find failed: " + e.getMessage());
        }
    }

    private void collectMatches(AccessibilityNodeInfo node, String field, String query,
        JSONArray out, int[] count) throws Exception {
        if (count[0] >= 80) return;
        String value = "";
        if ("text".equals(field) && node.getText() != null) value = node.getText().toString();
        else if ("description".equals(field) && node.getContentDescription() != null) value = node.getContentDescription().toString();
        else if ("resourceId".equals(field) && node.getViewIdResourceName() != null) value = node.getViewIdResourceName();
        if (!value.isEmpty() && value.toLowerCase().contains(query)) {
            Rect r = new Rect();
            node.getBoundsInScreen(r);
            JSONObject e = new JSONObject();
            e.put("value", value);
            e.put("resourceId", node.getViewIdResourceName() == null ? "" : node.getViewIdResourceName());
            e.put("className", String.valueOf(node.getClassName()));
            e.put("clickable", node.isClickable());
            e.put("editable", node.isEditable());
            e.put("x", r.left); e.put("y", r.top);
            e.put("width", r.width()); e.put("height", r.height());
            out.put(e); count[0]++;
        }
        for (int i = 0; i < node.getChildCount() && count[0] < 80; i++) {
            AccessibilityNodeInfo child = node.getChild(i);
            if (child != null) {
                collectMatches(child, field, query, out, count);
                child.recycle();
            }
        }
    }

    private void clearFocusedText(String requestId) {
        typeText(requestId, "");
    }

    private void clipboardSet(String requestId, String text) {
        try {
            android.content.ClipboardManager cm =
                (android.content.ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
            cm.setPrimaryClip(android.content.ClipData.newPlainText("Thomas", text));
            sendResult(requestId, new JSONObject(), null);
        } catch (Exception e) {
            sendResult(requestId, null, "clipboard set failed: " + e.getMessage());
        }
    }

    private void clipboardGet(String requestId) {
        try {
            android.content.ClipboardManager cm =
                (android.content.ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
            String text = "";
            if (cm.hasPrimaryClip() && cm.getPrimaryClip().getItemCount() > 0) {
                CharSequence cs = cm.getPrimaryClip().getItemAt(0).coerceToText(this);
                text = cs == null ? "" : cs.toString();
            }
            JSONObject out = new JSONObject();
            out.put("text", text);
            sendResult(requestId, out, null);
        } catch (Exception e) {
            sendResult(requestId, null, "clipboard get failed: " + e.getMessage());
        }
    }

    private void waitForText(String requestId, JSONObject p) {
        String field = p.optString("field", "text");
        String query = p.optString("query", "");
        long timeout = Math.max(250, Math.min(15000, p.optLong("timeoutMs", 5000)));
        new Thread(() -> {
            long end = android.os.SystemClock.uptimeMillis() + timeout;
            while (android.os.SystemClock.uptimeMillis() < end) {
                AccessibilityNodeInfo node = findNode(field, query);
                if (node != null) {
                    node.recycle();
                    try {
                        JSONObject out = new JSONObject();
                        out.put("found", true);
                        out.put("field", field);
                        out.put("query", query);
                        sendResult(requestId, out, null);
                    } catch (Exception e) {
                        sendResult(requestId, null, "wait result failed");
                    }
                    return;
                }
                try { Thread.sleep(250); } catch (InterruptedException ignored) { Thread.currentThread().interrupt(); break; }
            }
            try {
                JSONObject out = new JSONObject();
                out.put("found", false);
                out.put("field", field);
                out.put("query", query);
                sendResult(requestId, out, null);
            } catch (Exception e) {
                sendResult(requestId, null, "wait result failed");
            }
        }, "thomas-mobile-wait").start();
    }

    private void scroll(String requestId, JSONObject p) {
        try {
            AccessibilityNodeInfo root = getRootInActiveWindow();
            if (root == null) {
                sendResult(requestId, null, "no active window");
                return;
            }
            boolean forward = !"backward".equalsIgnoreCase(p.optString("direction", "forward"));
            boolean ok = performScroll(root, forward, new boolean[]{false});
            root.recycle();
            sendResult(requestId, new JSONObject(), ok ? null : "no scrollable node");
        } catch (Exception e) {
            sendResult(requestId, null, "scroll failed: " + e.getMessage());
        }
    }

    private boolean performScroll(AccessibilityNodeInfo node, boolean forward, boolean[] done) {
        if (node == null || done[0]) return false;
        if (node.isScrollable()) {
            done[0] = node.performAction(
                forward ? AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                        : AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD);
            if (done[0]) return true;
        }
        for (int i = 0; i < node.getChildCount() && !done[0]; i++) {
            AccessibilityNodeInfo child = node.getChild(i);
            if (child != null) {
                performScroll(child, forward, done);
                child.recycle();
            }
        }
        return done[0];
    }


}
