package ai.storyteller.wordcraft;

import android.os.Bundle;
import android.os.Process;
import android.view.View;

import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

import com.google.androidgamesdk.GameActivity;

/**
 * The app's activity: Google's GameActivity (it loads libwordcraft_android.so, named in the
 * manifest, and runs android_main on a thread of its own) plus one thing: the window insets.
 *
 * GameActivity draws edge to edge, behind the status bar, the navigation bar or taskbar, the
 * display cutout and the on-screen keyboard. The native glue doesn't pass the inset sizes on, so
 * the editor would be drawn under them. Here they become padding around the drawing surface: the
 * surface shrinks to the free area, and the editor simply sees a smaller window (above the keyboard
 * while it is up).
 *
 * The activity also ends the process with itself. The native side can run only once per process
 * (its windowing library allows one event loop, and does not stop it when the activity is
 * destroyed), so an activity started again in a process that lived on would fail at once; and
 * GameActivity's own onDestroy waits for the native thread to return, which that library never
 * does for a destroy the system started. WordCraft is one activity with no services, and nothing
 * is unsaved at this point: closing the app saved or discarded the document first, and the recovery
 * copy of unsaved work is written while the person types, not when the app is left.
 */
public class MainActivity extends GameActivity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        View content = findViewById(android.R.id.content);
        ViewCompat.setOnApplyWindowInsetsListener(content, (view, insets) -> {
            Insets free = insets.getInsets(
                    WindowInsetsCompat.Type.systemBars()
                            | WindowInsetsCompat.Type.displayCutout()
                            | WindowInsetsCompat.Type.ime());
            view.setPadding(free.left, free.top, free.right, free.bottom);
            return WindowInsetsCompat.CONSUMED;
        });
        ViewCompat.requestApplyInsets(content);
    }

    @Override
    protected void onDestroy() {
        // Before super.onDestroy(), which blocks until the native thread returns.
        Process.killProcess(Process.myPid());
        super.onDestroy();
    }
}
