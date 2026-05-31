package bike.ridi.modules.router

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import org.json.JSONObject
import uniffi.ridi_router_mobile.generateRouteJson as generateRouteJsonRust

class RidiRouterModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("RidiRouter")

    AsyncFunction("generateRouteJson") { tilesDir: String, requestJson: String ->
      try {
        generateRouteJsonRust(tilesDir, requestJson)
      } catch (throwable: Throwable) {
        nativeBindingFailedEnvelope(throwable)
      }
    }
  }

  private fun nativeBindingFailedEnvelope(throwable: Throwable): String {
    return "{" +
      "\"ok\":false," +
      "\"error\":{" +
      "\"code\":\"native_binding_failed\"," +
      "\"message\":\"Native routing call failed\"," +
      "\"details\":" + JSONObject.quote(throwable.toString()) +
      "}" +
      "}"
  }
}
