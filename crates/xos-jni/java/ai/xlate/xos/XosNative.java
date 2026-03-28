package ai.xlate.xos;

import java.nio.ByteBuffer;

/**
 * JNI bridge to the xos engine. Native library name: {@code xos_jni}
 * ({@code xos_jni.dll} / {@code libxos_jni.so} / {@code libxos_jni.dylib}).
 * <p>
 * Call {@link #initLibrary(String)} once before any other method, with the absolute path to the
 * native library, <em>or</em> ensure {@code xos_jni} is on {@code java.library.path} and use
 * {@link System#loadLibrary(String)} yourself before touching this class.
 * <p>
 * After {@link #resize}, any previously obtained {@link #getFrameBuffer()} may be invalid; obtain
 * a fresh direct buffer.
 */
public final class XosNative {

    private static volatile boolean loaded;

    private XosNative() {}

    /**
     * Loads {@code xos_jni} from an absolute filesystem path (recommended for modded games).
     */
    public static synchronized void initLibrary(String absolutePathToNativeLibrary) {
        if (loaded) {
            return;
        }
        System.load(absolutePathToNativeLibrary);
        loaded = true;
    }

    /**
     * Loads {@code xos_jni} from {@code java.library.path} (e.g. {@code -Djava.library.path=...}).
     */
    public static synchronized void initLibraryFromPath() {
        if (loaded) {
            return;
        }
        System.loadLibrary("xos_jni");
        loaded = true;
    }

    /**
     * Low-level JNI smoke test: returns a string allocated on the Rust side.
     * Call after {@link #initLibraryFromPath()} or {@link #initLibrary(String)}.
     */
    public static native String ping();

    /**
     * Hello-world check for mods: same as {@link #ping()} — verifies xos_jni is loaded and JNI works.
     */
    public static String helloWorld() {
        return ping();
    }

    public static native void init(int width, int height);

    public static native void shutdown();

    public static native void tick();

    /**
     * Direct buffer over the engine RGBA framebuffer (native memory). Not valid after
     * {@link #shutdown} or if the native side reallocates; call again after {@link #resize}.
     */
    public static native ByteBuffer getFrameBuffer();

    public static native void resize(int width, int height);

    public static native void onMouseMove(float x, float y);

    /** Button: 0 = left, 1 = right (matches common Minecraft / LWJGL conventions). */
    public static native void onMouseDown(int button);

    public static native void onMouseUp(int button);

    public static native void onScroll(float dx, float dy);

    /** Unicode code point (e.g. from {@link Character#codePointAt(CharSequence, int)}). */
    public static native void onKeyChar(int codepoint);
}
