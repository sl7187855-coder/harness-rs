package com.thomas.mobileagent;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.hardware.display.DisplayManager;
import android.hardware.display.VirtualDisplay;
import android.media.Image;
import android.media.ImageReader;
import android.media.projection.MediaProjection;
import android.media.projection.MediaProjectionManager;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.IBinder;
import android.util.DisplayMetrics;
import android.view.WindowManager;
import android.content.pm.ServiceInfo;

import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;

public class ScreenCaptureService extends Service {
    public interface Callback {
        void onSuccess(byte[] jpeg);
        void onError(String error);
    }

    private static final int NOTIFICATION_ID = 4711;
    private static final String CHANNEL_ID = "thomas_mobile_capture";
    private static MediaProjection projection;
    private static HandlerThread thread;
    private static Handler handler;

    public static void startWithPermission(Context context, int resultCode, Intent data) {
        Intent i = new Intent(context, ScreenCaptureService.class);
        i.putExtra("result_code", resultCode);
        i.putExtra("result_data", data);
        if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(i);
        else context.startService(i);
    }

    @Override
    public void onCreate() {
        super.onCreate();
        thread = new HandlerThread("thomas-capture");
        thread.start();
        handler = new Handler(thread.getLooper());
        createChannel();
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        try {
            Notification n = buildNotification();
            if (Build.VERSION.SDK_INT >= 29) {
                startForeground(NOTIFICATION_ID, n,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION);
            } else {
                startForeground(NOTIFICATION_ID, n);
            }

            if (intent != null && intent.hasExtra("result_data")) {
                Intent data = intent.getParcelableExtra("result_data");
                int resultCode = intent.getIntExtra("result_code", 0);
                MediaProjectionManager manager =
                    (MediaProjectionManager) getSystemService(MEDIA_PROJECTION_SERVICE);
                if (data != null && resultCode != 0) {
                    projection = manager.getMediaProjection(resultCode, data);
                    if (projection != null) {
                        projection.registerCallback(new MediaProjection.Callback() {
                            @Override public void onStop() { projection = null; }
                        }, handler);
                    }
                }
            }
        } catch (Throwable ignored) {
        }
        return START_STICKY;
    }

    public static void captureJpeg(Callback callback) {
        if (handler == null) {
            callback.onError("capture service not ready");
            return;
        }
        handler.post(() -> captureOnWorker(callback));
    }

    private static void captureOnWorker(Callback callback) {
        MediaProjection p = projection;
        if (p == null) {
            callback.onError("screen capture permission not active");
            return;
        }

        try {
            WindowManager wm = (WindowManager) INSTANCE.getSystemService(WINDOW_SERVICE);
            DisplayMetrics dm = new DisplayMetrics();
            wm.getDefaultDisplay().getRealMetrics(dm);
            int width = Math.max(1, dm.widthPixels);
            int height = Math.max(1, dm.heightPixels);

            ImageReader reader = ImageReader.newInstance(
                width, height, android.graphics.PixelFormat.RGBA_8888, 2);

            VirtualDisplay display = p.createVirtualDisplay(
                "ThomasMobileCapture",
                width, height, dm.densityDpi,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR,
                reader.getSurface(), null, handler);

            final boolean[] finished = {false};
            final Runnable timeout = () -> {
                if (finished[0]) return;
                finished[0] = true;
                try { display.release(); } catch (Throwable ignored) {}
                try { reader.close(); } catch (Throwable ignored) {}
                callback.onError("screen capture timeout");
            };

            handler.postDelayed(timeout, 2500);

            reader.setOnImageAvailableListener(r -> {
                if (finished[0]) return;
                Image image = null;
                try {
                    image = r.acquireLatestImage();
                    if (image == null) return;
                    Image.Plane plane = image.getPlanes()[0];
                    ByteBuffer buffer = plane.getBuffer();
                    int pixelStride = plane.getPixelStride();
                    int rowStride = plane.getRowStride();
                    int rowPadding = rowStride - pixelStride * width;
                    int paddedWidth = width + rowPadding / pixelStride;

                    Bitmap bitmap = Bitmap.createBitmap(
                        paddedWidth, height, Bitmap.Config.ARGB_8888);
                    buffer.rewind();
                    bitmap.copyPixelsFromBuffer(buffer);

                    Bitmap cropped = bitmap;
                    if (paddedWidth != width) {
                        cropped = Bitmap.createBitmap(bitmap, 0, 0, width, height);
                        bitmap.recycle();
                    }

                    ByteArrayOutputStream out = new ByteArrayOutputStream(256 * 1024);
                    cropped.compress(Bitmap.CompressFormat.JPEG, 72, out);
                    byte[] jpeg = out.toByteArray();
                    cropped.recycle();

                    finished[0] = true;
                    handler.removeCallbacks(timeout);
                    try { display.release(); } catch (Throwable ignored) {}
                    try { reader.close(); } catch (Throwable ignored) {}
                    callback.onSuccess(jpeg);
                } catch (Throwable e) {
                    finished[0] = true;
                    handler.removeCallbacks(timeout);
                    try { display.release(); } catch (Throwable ignored) {}
                    try { reader.close(); } catch (Throwable ignored) {}
                    callback.onError("capture failed: " + e.getClass().getSimpleName());
                } finally {
                    if (image != null) image.close();
                }
            }, handler);
        } catch (Throwable e) {
            callback.onError("capture setup failed: " + e.getMessage());
        }
    }

    private static ScreenCaptureService INSTANCE;

    private void createChannel() {
        INSTANCE = this;
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID, "Thomas Mobile", NotificationManager.IMPORTANCE_LOW);
            NotificationManager nm = getSystemService(NotificationManager.class);
            nm.createNotificationChannel(channel);
        }
    }

    private Notification buildNotification() {
        Notification.Builder b = Build.VERSION.SDK_INT >= 26
            ? new Notification.Builder(this, CHANNEL_ID)
            : new Notification.Builder(this);
        return b.setContentTitle("Thomas Mobile")
            .setContentText("Captura de tela autorizada")
            .setSmallIcon(android.R.drawable.ic_menu_view)
            .setOngoing(true)
            .build();
    }

    @Override public IBinder onBind(Intent intent) { return null; }

    @Override public void onDestroy() {
        projection = null;
        INSTANCE = null;
        if (thread != null) thread.quitSafely();
        handler = null;
        super.onDestroy();
    }
}
