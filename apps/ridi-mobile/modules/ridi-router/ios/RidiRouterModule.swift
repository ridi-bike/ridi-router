import ExpoModulesCore

public class RidiRouterModule: Module {
  public func definition() -> ModuleDefinition {
    Name("RidiRouter")

    AsyncFunction("generateRouteJson") { (tilesDir: String, requestJson: String) -> String in
      do {
        return try generateRouteJson(tilesDir: tilesDir, requestJson: requestJson)
      } catch {
        return Self.nativeBindingFailedEnvelope(error)
      }
    }
  }

  private static func nativeBindingFailedEnvelope(_ error: Error) -> String {
    let details = String(describing: error)
    let envelope: [String: Any] = [
      "ok": false,
      "error": [
        "code": "native_binding_failed",
        "message": "Native routing call failed",
        "details": details,
      ],
    ]

    guard JSONSerialization.isValidJSONObject(envelope),
      let data = try? JSONSerialization.data(withJSONObject: envelope),
      let json = String(data: data, encoding: .utf8)
    else {
      return "{\"ok\":false,\"error\":{\"code\":\"unknown\",\"message\":\"Native routing call failed\"}}"
    }

    return json
  }
}
