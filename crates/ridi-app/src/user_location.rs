use crate::space::GpsCoord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserLocationStatus {
    Idle,
    Requesting,
    Available,
    Denied,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UserLocationState {
    pub status: UserLocationStatus,
    pub location: Option<GpsCoord>,
}

impl Default for UserLocationState {
    fn default() -> Self {
        Self {
            status: UserLocationStatus::Idle,
            location: None,
        }
    }
}

impl UserLocationState {
    pub fn request(&mut self) {
        self.status = UserLocationStatus::Requesting;
        request_user_location();
    }

    pub fn refresh(&mut self) {
        *self = latest_user_location_state();
    }

    pub fn label(self) -> &'static str {
        match self.status {
            UserLocationStatus::Idle => "Locate",
            UserLocationStatus::Requesting => "Locating...",
            UserLocationStatus::Available => "Located",
            UserLocationStatus::Denied => "Denied",
            UserLocationStatus::Unsupported => "Unsupported",
            UserLocationStatus::Error => "Location error",
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::{GpsCoord, UserLocationState, UserLocationStatus};
    use std::cell::RefCell;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;

    thread_local! {
        static STATE: RefCell<UserLocationState> = RefCell::new(UserLocationState::default());
    }

    pub fn request_user_location() {
        STATE.with(|state| {
            state.borrow_mut().status = UserLocationStatus::Requesting;
        });

        let Some(window) = web_sys::window() else {
            set_status(UserLocationStatus::Unsupported);
            return;
        };

        let geolocation = match window.navigator().geolocation() {
            Ok(geolocation) => geolocation,
            Err(_) => {
                set_status(UserLocationStatus::Unsupported);
                return;
            }
        };

        let success = Closure::<dyn FnMut(JsValue)>::new(|position| {
            let location = parse_position(position);
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                match location {
                    Some(location) => {
                        state.status = UserLocationStatus::Available;
                        state.location = Some(location);
                    }
                    None => {
                        state.status = UserLocationStatus::Error;
                    }
                }
            });
        });

        let error = Closure::<dyn FnMut(JsValue)>::new(|error| {
            let status = permission_error_code(error)
                .map(|code| {
                    if code == 1 {
                        UserLocationStatus::Denied
                    } else {
                        UserLocationStatus::Error
                    }
                })
                .unwrap_or(UserLocationStatus::Error);
            set_status(status);
        });

        if geolocation
            .get_current_position_with_error_callback(
                success.as_ref().unchecked_ref(),
                Some(error.as_ref().unchecked_ref()),
            )
            .is_err()
        {
            set_status(UserLocationStatus::Error);
            return;
        }

        success.forget();
        error.forget();
    }

    pub fn latest_user_location_state() -> UserLocationState {
        STATE.with(|state| *state.borrow())
    }

    fn set_status(status: UserLocationStatus) {
        STATE.with(|state| {
            state.borrow_mut().status = status;
        });
    }

    fn parse_position(position: JsValue) -> Option<GpsCoord> {
        let coords = js_sys::Reflect::get(&position, &JsValue::from_str("coords")).ok()?;
        let lat = js_sys::Reflect::get(&coords, &JsValue::from_str("latitude"))
            .ok()?
            .as_f64()?;
        let lon = js_sys::Reflect::get(&coords, &JsValue::from_str("longitude"))
            .ok()?
            .as_f64()?;
        Some(GpsCoord { lat, lon })
    }

    fn permission_error_code(error: JsValue) -> Option<u16> {
        js_sys::Reflect::get(&error, &JsValue::from_str("code"))
            .ok()?
            .as_f64()
            .map(|code| code as u16)
    }
}

#[cfg(target_os = "android")]
mod platform {
    use super::{GpsCoord, UserLocationState, UserLocationStatus};
    use jni::objects::{JObject, JValue};
    use jni::sys::{jobject, JNIEnv as RawJNIEnv};
    use jni::JNIEnv;
    use macroquad::miniquad::native::android::{attach_jni_env, ACTIVITY};
    use std::sync::Mutex;

    const FINE_LOCATION: &str = "android.permission.ACCESS_FINE_LOCATION";
    const COARSE_LOCATION: &str = "android.permission.ACCESS_COARSE_LOCATION";
    const LOCATION_SERVICE: &str = "location";
    const PROVIDERS: [&str; 2] = ["gps", "network"];
    const PERMISSION_GRANTED: i32 = 0;
    const LOCATION_PERMISSION_REQUEST: i32 = 42;

    static STATE: Mutex<UserLocationState> = Mutex::new(UserLocationState {
        status: UserLocationStatus::Idle,
        location: None,
    });

    pub fn request_user_location() {
        match request_permission_and_read_location() {
            AndroidLocationResult::Available(location) => {
                set_state(UserLocationStatus::Available, Some(location))
            }
            AndroidLocationResult::WaitingForPermission => {
                set_state(UserLocationStatus::Requesting, None)
            }
            AndroidLocationResult::Unavailable => set_state(UserLocationStatus::Requesting, None),
            AndroidLocationResult::Error => set_state(UserLocationStatus::Error, None),
        }
    }

    pub fn latest_user_location_state() -> UserLocationState {
        let previous = STATE.lock().map(|state| *state).unwrap_or_default();
        if !matches!(
            previous.status,
            UserLocationStatus::Requesting | UserLocationStatus::Available
        ) {
            return previous;
        }

        match read_location_if_permitted() {
            AndroidLocationResult::Available(location) => {
                set_state(UserLocationStatus::Available, Some(location));
            }
            AndroidLocationResult::WaitingForPermission => {
                set_state(UserLocationStatus::Requesting, previous.location);
            }
            AndroidLocationResult::Unavailable => {
                set_state(previous.status, previous.location);
            }
            AndroidLocationResult::Error => {
                set_state(UserLocationStatus::Error, previous.location);
            }
        }

        STATE.lock().map(|state| *state).unwrap_or_default()
    }

    enum AndroidLocationResult {
        Available(GpsCoord),
        WaitingForPermission,
        Unavailable,
        Error,
    }

    fn request_permission_and_read_location() -> AndroidLocationResult {
        let Ok(mut env) = android_env() else {
            return AndroidLocationResult::Error;
        };

        let activity = unsafe { JObject::from_raw(ACTIVITY as jobject) };
        let permissions_granted = has_location_permission(&mut env, &activity).unwrap_or(false);
        if !permissions_granted {
            let _ = request_location_permissions(&mut env, &activity);
            std::mem::forget(activity);
            return AndroidLocationResult::WaitingForPermission;
        }

        let result = read_last_known_location(&mut env, &activity);
        std::mem::forget(activity);
        result
    }

    fn read_location_if_permitted() -> AndroidLocationResult {
        let Ok(mut env) = android_env() else {
            return AndroidLocationResult::Error;
        };

        let activity = unsafe { JObject::from_raw(ACTIVITY as jobject) };
        let permissions_granted = has_location_permission(&mut env, &activity).unwrap_or(false);
        let result = if permissions_granted {
            read_last_known_location(&mut env, &activity)
        } else {
            AndroidLocationResult::WaitingForPermission
        };
        std::mem::forget(activity);
        result
    }

    fn android_env() -> Result<JNIEnv<'static>, jni::errors::Error> {
        unsafe { JNIEnv::from_raw(attach_jni_env() as *mut RawJNIEnv) }
    }

    fn has_location_permission(
        env: &mut JNIEnv<'_>,
        activity: &JObject<'_>,
    ) -> Result<bool, jni::errors::Error> {
        let fine = env.new_string(FINE_LOCATION)?;
        let coarse = env.new_string(COARSE_LOCATION)?;
        let fine_status = env
            .call_method(
                activity,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[JValue::Object(&JObject::from(fine))],
            )?
            .i()?;
        let coarse_status = env
            .call_method(
                activity,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[JValue::Object(&JObject::from(coarse))],
            )?
            .i()?;

        Ok(fine_status == PERMISSION_GRANTED || coarse_status == PERMISSION_GRANTED)
    }

    fn request_location_permissions(
        env: &mut JNIEnv<'_>,
        activity: &JObject<'_>,
    ) -> Result<(), jni::errors::Error> {
        let string_class = env.find_class("java/lang/String")?;
        let permissions = env.new_object_array(2, string_class, JObject::null())?;
        let fine = env.new_string(FINE_LOCATION)?;
        let coarse = env.new_string(COARSE_LOCATION)?;
        env.set_object_array_element(&permissions, 0, fine)?;
        env.set_object_array_element(&permissions, 1, coarse)?;
        let permissions = JObject::from(permissions);

        env.call_method(
            activity,
            "requestPermissions",
            "([Ljava/lang/String;I)V",
            &[
                JValue::Object(&permissions),
                JValue::Int(LOCATION_PERMISSION_REQUEST),
            ],
        )?;
        Ok(())
    }

    fn read_last_known_location(
        env: &mut JNIEnv<'_>,
        activity: &JObject<'_>,
    ) -> AndroidLocationResult {
        let Ok(service_name) = env.new_string(LOCATION_SERVICE) else {
            return AndroidLocationResult::Error;
        };
        let Ok(location_manager) = env
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&JObject::from(service_name))],
            )
            .and_then(|value| value.l())
        else {
            return AndroidLocationResult::Error;
        };

        for provider in PROVIDERS {
            let Ok(provider) = env.new_string(provider) else {
                continue;
            };
            let Ok(location) = env
                .call_method(
                    &location_manager,
                    "getLastKnownLocation",
                    "(Ljava/lang/String;)Landroid/location/Location;",
                    &[JValue::Object(&JObject::from(provider))],
                )
                .and_then(|value| value.l())
            else {
                continue;
            };

            if location.as_raw().is_null() {
                continue;
            }

            let Ok(lat) = env
                .call_method(&location, "getLatitude", "()D", &[])
                .and_then(|value| value.d())
            else {
                continue;
            };
            let Ok(lon) = env
                .call_method(&location, "getLongitude", "()D", &[])
                .and_then(|value| value.d())
            else {
                continue;
            };

            return AndroidLocationResult::Available(GpsCoord { lat, lon });
        }

        AndroidLocationResult::Unavailable
    }

    fn set_state(status: UserLocationStatus, location: Option<GpsCoord>) {
        if let Ok(mut state) = STATE.lock() {
            state.status = status;
            state.location = location.or(state.location);
        }
    }
}

#[cfg(target_os = "ios")]
mod platform {
    use super::{GpsCoord, UserLocationState, UserLocationStatus};
    use objc::runtime::{Object, BOOL, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::Mutex;

    #[link(name = "CoreLocation", kind = "framework")]
    extern "C" {}

    const AUTH_NOT_DETERMINED: i32 = 0;
    const AUTH_RESTRICTED: i32 = 1;
    const AUTH_DENIED: i32 = 2;
    const AUTH_AUTHORIZED_ALWAYS: i32 = 3;
    const AUTH_AUTHORIZED_WHEN_IN_USE: i32 = 4;

    static STATE: Mutex<UserLocationState> = Mutex::new(UserLocationState {
        status: UserLocationStatus::Idle,
        location: None,
    });
    static MANAGER: Mutex<Option<usize>> = Mutex::new(None);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CLLocationCoordinate2D {
        latitude: f64,
        longitude: f64,
    }

    pub fn request_user_location() {
        match request_permission_and_read_location() {
            IosLocationResult::Available(location) => {
                set_state(UserLocationStatus::Available, Some(location))
            }
            IosLocationResult::WaitingForPermission => {
                set_state(UserLocationStatus::Requesting, None)
            }
            IosLocationResult::Unavailable => set_state(UserLocationStatus::Requesting, None),
            IosLocationResult::Denied => set_state(UserLocationStatus::Denied, None),
            IosLocationResult::Unsupported => set_state(UserLocationStatus::Unsupported, None),
            IosLocationResult::Error => set_state(UserLocationStatus::Error, None),
        }
    }

    pub fn latest_user_location_state() -> UserLocationState {
        let previous = STATE.lock().map(|state| *state).unwrap_or_default();
        if !matches!(
            previous.status,
            UserLocationStatus::Requesting | UserLocationStatus::Available
        ) {
            return previous;
        }

        match read_location_if_permitted() {
            IosLocationResult::Available(location) => {
                set_state(UserLocationStatus::Available, Some(location));
            }
            IosLocationResult::WaitingForPermission => {
                set_state(UserLocationStatus::Requesting, previous.location);
            }
            IosLocationResult::Unavailable => {
                set_state(previous.status, previous.location);
            }
            IosLocationResult::Denied => {
                set_state(UserLocationStatus::Denied, previous.location);
            }
            IosLocationResult::Unsupported => {
                set_state(UserLocationStatus::Unsupported, previous.location);
            }
            IosLocationResult::Error => {
                set_state(UserLocationStatus::Error, previous.location);
            }
        }

        STATE.lock().map(|state| *state).unwrap_or_default()
    }

    enum IosLocationResult {
        Available(GpsCoord),
        WaitingForPermission,
        Unavailable,
        Denied,
        Unsupported,
        Error,
    }

    fn request_permission_and_read_location() -> IosLocationResult {
        unsafe {
            if !location_services_enabled() {
                return IosLocationResult::Unsupported;
            }

            let manager = location_manager();
            if manager.is_null() {
                return IosLocationResult::Error;
            }

            match authorization_status(manager) {
                AUTH_AUTHORIZED_ALWAYS | AUTH_AUTHORIZED_WHEN_IN_USE => {
                    start_updating_location(manager);
                    read_current_location(manager)
                }
                AUTH_NOT_DETERMINED => {
                    let _: () = msg_send![manager, requestWhenInUseAuthorization];
                    start_updating_location(manager);
                    IosLocationResult::WaitingForPermission
                }
                AUTH_DENIED | AUTH_RESTRICTED => IosLocationResult::Denied,
                _ => IosLocationResult::Error,
            }
        }
    }

    fn read_location_if_permitted() -> IosLocationResult {
        unsafe {
            if !location_services_enabled() {
                return IosLocationResult::Unsupported;
            }

            let manager = location_manager();
            if manager.is_null() {
                return IosLocationResult::Error;
            }

            match authorization_status(manager) {
                AUTH_AUTHORIZED_ALWAYS | AUTH_AUTHORIZED_WHEN_IN_USE => {
                    start_updating_location(manager);
                    read_current_location(manager)
                }
                AUTH_NOT_DETERMINED => IosLocationResult::WaitingForPermission,
                AUTH_DENIED | AUTH_RESTRICTED => IosLocationResult::Denied,
                _ => IosLocationResult::Error,
            }
        }
    }

    unsafe fn location_services_enabled() -> bool {
        let enabled: BOOL = msg_send![class!(CLLocationManager), locationServicesEnabled];
        enabled == YES
    }

    unsafe fn location_manager() -> *mut Object {
        let mut manager_guard = MANAGER.lock().ok();
        if let Some(Some(manager)) = manager_guard.as_ref().map(|guard| **guard) {
            return manager as *mut Object;
        }

        let manager: *mut Object = msg_send![class!(CLLocationManager), alloc];
        let manager: *mut Object = msg_send![manager, init];
        if manager.is_null() {
            return manager;
        }

        let _: () = msg_send![manager, setPausesLocationUpdatesAutomatically: false];
        if let Some(manager_guard) = manager_guard.as_mut() {
            **manager_guard = Some(manager as usize);
        }
        manager
    }

    unsafe fn authorization_status(manager: *mut Object) -> i32 {
        let responds: BOOL = msg_send![manager, respondsToSelector: sel!(authorizationStatus)];
        if responds == YES {
            msg_send![manager, authorizationStatus]
        } else {
            msg_send![class!(CLLocationManager), authorizationStatus]
        }
    }

    unsafe fn start_updating_location(manager: *mut Object) {
        let _: () = msg_send![manager, startUpdatingLocation];
    }

    unsafe fn read_current_location(manager: *mut Object) -> IosLocationResult {
        let location: *mut Object = msg_send![manager, location];
        if location.is_null() {
            return IosLocationResult::Unavailable;
        }

        let coordinate: CLLocationCoordinate2D = msg_send![location, coordinate];
        if !coordinate.latitude.is_finite() || !coordinate.longitude.is_finite() {
            return IosLocationResult::Unavailable;
        }

        IosLocationResult::Available(GpsCoord {
            lat: coordinate.latitude,
            lon: coordinate.longitude,
        })
    }

    fn set_state(status: UserLocationStatus, location: Option<GpsCoord>) {
        if let Ok(mut state) = STATE.lock() {
            state.status = status;
            state.location = location.or(state.location);
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
mod platform {
    use super::{UserLocationState, UserLocationStatus};
    use std::sync::Mutex;

    static STATE: Mutex<UserLocationState> = Mutex::new(UserLocationState {
        status: UserLocationStatus::Idle,
        location: None,
    });

    pub fn request_user_location() {
        if let Ok(mut state) = STATE.lock() {
            state.status = UserLocationStatus::Unsupported;
        }
    }

    pub fn latest_user_location_state() -> UserLocationState {
        STATE
            .lock()
            .map(|state| *state)
            .unwrap_or(UserLocationState {
                status: UserLocationStatus::Error,
                location: None,
            })
    }
}

use platform::{latest_user_location_state, request_user_location};
