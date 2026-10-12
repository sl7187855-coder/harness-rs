# Thomas Mobile Agent Android app

- Application ID: com.thomas.mobileagent
- Version: 0.5.3 (versionCode 12)
- Language: Java 17
- Android Gradle Plugin: 8.7.3
- Gradle wrapper: 8.9
- Compile SDK / build tools: 35 / 35.0.0
- Minimum SDK: 26 (Android 8.0)
- Target SDK: 35
- Networking dependency: OkHttp 4.12.0

## Build

Install Java 17 and Android SDK packages platforms;android-35 and build-tools;35.0.0. On Linux:

~~~sh
export ANDROID_HOME="$HOME/Android/Sdk"   # change to your SDK location
export ANDROID_SDK_ROOT="$ANDROID_HOME"
./gradlew --no-daemon --max-workers=1 assembleDebug
~~~

The output is app/build/outputs/apk/debug/app-debug.apk. The repository test artifact is at ../releases/Thomas-Mobile-Agent-0.5.3-debug.apk; compare its .sha256 file after download. Current artifact SHA-256: 625a5a030981a61ca4a23503c3741b9e5cd0d2399950beeb1ea2e3a02af3333b.

The included APK is debug-signed. A debug APK is appropriate for development, but it is not a production release and cannot be assumed to update an APK signed by another key. Release builds should use a separately managed signing key that is never committed to the repository or injected into public CI logs.

## First launch

1. Install the APK you built or verified.
2. Enter the HTTPS/WSS address of a relay you trust. The endpoint field starts blank intentionally; the expected WebSocket path is /agent.
3. Enter the account credentials provisioned by the relay administrator.
4. Grant ordinary Android runtime permissions only for the capabilities you intend to use.
5. Enable the Thomas Accessibility Service explicitly in Android Settings if remote UI interaction is needed.
6. Grant screen-capture consent from Android's system prompt if screenshots are required. Consent is controlled by Android and may need to be renewed.
7. Keep the device unlocked during initial pairing and verify the expected device identity in the relay's private device list.

## Capabilities in the current app

The wire protocol reports supported actions including accessibility observation, tap/long-press/swipe, text input and clearing, Back/Home/Recents, notification shade/Quick Settings, screen lock/wake, app launch/open URL, settings panels, screenshot capture, text search/wait, clipboard read/write, battery/device information, screen state, brightness, volume and media control. Actual availability depends on Android version, granted permissions, service state and device policy. Reboot requires Device Owner privileges and must not be presented as generally available.

## Permissions and privacy

The manifest requests networking, audio/camera/media, notifications, Bluetooth/nearby-device and foreground-service capabilities, plus special Android settings for overlay, battery optimization and configuration. Not every permission is needed for every action. Users should grant the minimum set required for their own use.

This source does not embed a live server certificate or endpoint. By default it relies on Android's system trust store. Use a publicly trusted TLS certificate for a public relay. For a private CA, provision only its reviewed public certificate through a private build process; never embed the private key.
