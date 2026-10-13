package ai.storyteller.wordcraft;

import android.os.Bundle;
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
}
