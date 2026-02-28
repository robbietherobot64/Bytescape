package meta.bytescape.app

import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    
    val decorView = window.decorView
    ViewCompat.setOnApplyWindowInsetsListener(decorView) { _, windowInsets ->
      val insets = windowInsets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val density = resources.displayMetrics.density
      val top = Math.round(insets.top.toFloat() / density)
      val bottom = Math.round(insets.bottom.toFloat() / density)
      val left = Math.round(insets.left.toFloat() / density)
      val right = Math.round(insets.right.toFloat() / density)
      
      val webView = findWebView(decorView)
      val js = """
        (function() {
          const root = document.documentElement;
          root.style.setProperty('--android-safe-area-top', '${top}px');
          root.style.setProperty('--android-safe-area-bottom', '${bottom}px');
          root.style.setProperty('--android-safe-area-left', '${left}px');
          root.style.setProperty('--android-safe-area-right', '${right}px');
          console.log('Android Safe Areas Restored: ' + ${top} + 'px');
        })();
      """.trimIndent()
      
      webView?.evaluateJavascript(js, null)
      windowInsets
    }
  }

  private fun findWebView(view: View): WebView? {
    if (view is WebView) { return view }
    if (view is ViewGroup) {
      for (i in 0 until view.childCount) {
        val result = findWebView(view.getChildAt(i))
        if (result != null) return result
      }
    }
    return null
  }
}
