use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;
use tauri_plugin_updater::UpdaterExt;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct VisualSettings {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue: f32,
}

impl Default for VisualSettings {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 100.0,
            saturation: 100.0,
            hue: 0.0,
        }
    }
}

impl VisualSettings {
    pub fn clamped(&self) -> Self {
        Self {
            brightness: self.brightness.clamp(-50.0, 50.0),
            contrast: self.contrast.clamp(50.0, 150.0),
            saturation: self.saturation.clamp(0.0, 200.0),
            hue: self.hue.clamp(-180.0, 180.0),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ApplyResult {
    pub gamma_applied: bool,
    pub nvapi_applied: bool,
    pub hue_applied: bool,
    pub message: String,
}

#[derive(Serialize)]
struct RuntimeInfo {
    platform: String,
    display_backend: String,
    nvidia_gpu_detected: bool,
}

#[tauri::command]
fn runtime_info() -> RuntimeInfo {
    let (platform, display_backend) = if cfg!(target_os = "windows") {
        ("Windows".to_string(), "Windows Display + NVAPI".to_string())
    } else {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "Unknown".to_string());
        ("Linux".to_string(), session)
    };

    let nvidia_gpu_detected = detect_nvidia_gpu();

    RuntimeInfo {
        platform,
        display_backend,
        nvidia_gpu_detected,
    }
}

#[tauri::command]
fn apply_visuals(settings: VisualSettings) -> ApplyResult {
    let settings = settings.clamped();

    #[cfg(windows)]
    {
        apply_visuals_windows(&settings)
    }

    #[cfg(target_os = "linux")]
    {
        apply_visuals_linux(&settings)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = settings;
        ApplyResult {
            gamma_applied: false,
            nvapi_applied: false,
            hue_applied: false,
            message: "Nieobsługiwany system operacyjny.".to_string(),
        }
    }
}

fn detect_nvidia_gpu() -> bool {
    #[cfg(target_os = "linux")]
    {
        Command::new("nvidia-smi")
            .arg("-L")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        use std::ptr;
        use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;

        unsafe {
            let name: Vec<u16> = "nvapi64.dll".encode_utf16().chain(Some(0)).collect();
            let handle = LoadLibraryW(name.as_ptr());
            handle != ptr::null_mut()
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        false
    }
}

#[cfg(target_os = "linux")]
fn apply_visuals_linux(settings: &VisualSettings) -> ApplyResult {
    let mut messages = Vec::new();
    let mut gamma_applied = false;
    let mut nvapi_applied = false;
    let hue_applied = false;

    if detect_nvidia_gpu() {
        let vibrance_target =
            (((settings.saturation - 100.0) / 100.0) * 512.0).clamp(-512.0, 512.0) as i32;
        let nvidia_settings_result = Command::new("nvidia-settings")
            .args(["-a", &format!("DigitalVibrance={}", vibrance_target)])
            .output();

        nvapi_applied = nvidia_settings_result
            .as_ref()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if nvapi_applied {
            messages.push(format!("DigitalVibrance: {}", vibrance_target));
        }
    }

    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "".to_string());

    if session_type == "wayland" {
        let br_val = ((settings.brightness / 100.0) + 1.0).clamp(0.1, 2.0);
        let co_val = (settings.contrast / 100.0).clamp(0.1, 2.0);

        let gammastep_result = Command::new("gammastep")
            .args([
                "-O",
                "6500",
                "-b",
                &format!("{br_val}"),
                "-c",
                &format!("{co_val}"),
            ])
            .output();

        gamma_applied = gammastep_result
            .as_ref()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if gamma_applied {
            messages.push(format!(
                "Gammastep (Wayland): Jasność={br_val:.2}, Kontrast={co_val:.2}"
            ));
        } else {
            messages.push(
                "Gammastep: Błąd (zainstaluj pakiet za pomocą 'sudo dnf install gammastep')"
                    .to_string(),
            );
        }
    } else {
        let primary_output = get_primary_output_xrandr();
        if let Some(output) = primary_output {
            let brightness_val = ((settings.brightness / 50.0) + 1.0).clamp(0.2, 3.0);
            let contrast_val = (settings.contrast / 100.0).clamp(0.2, 3.0);
            let gamma_val = format!("{}:{}:{}", contrast_val, contrast_val, contrast_val);

            let xrandr_result = Command::new("xrandr")
                .args([
                    "--output",
                    &output,
                    "--brightness",
                    &brightness_val.to_string(),
                    "--gamma",
                    &gamma_val,
                ])
                .output();

            gamma_applied = xrandr_result
                .as_ref()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if gamma_applied {
                messages.push(format!(
                    "Xrandr (X11): {} jasność={:.2}",
                    output, brightness_val
                ));
            }
        } else {
            messages.push("Nie znaleziono ekranu Xrandr".to_string());
        }
    }

    ApplyResult {
        gamma_applied,
        nvapi_applied,
        hue_applied,
        message: if messages.is_empty() {
            "Brak zmian".to_string()
        } else {
            messages.join(" | ")
        },
    }
}

#[cfg(target_os = "linux")]
fn get_primary_output_xrandr() -> Option<String> {
    let output = Command::new("xrandr").arg("--query").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains(" connected") && !line.contains("disconnected") {
            return line.split_whitespace().next().map(|s| s.to_string());
        }
    }
    None
}

#[cfg(windows)]
fn apply_visuals_windows(settings: &VisualSettings) -> ApplyResult {
    let gamma_applied = unsafe { apply_gamma_ramp(settings) };
    let nvapi_result = unsafe { nvapi::try_apply_dvc(settings.saturation, settings.hue) };
    let (nvapi_applied, hue_applied) = nvapi_result.unwrap_or((false, false));

    let mut messages = Vec::new();
    if gamma_applied {
        messages.push("GDI Gamma: OK".to_string());
    }
    if nvapi_applied {
        messages.push("NVAPI Vibrance: OK".to_string());
    }
    if hue_applied {
        messages.push("NVAPI Hue: OK".to_string());
    }

    ApplyResult {
        gamma_applied,
        nvapi_applied,
        hue_applied,
        message: if messages.is_empty() {
            "Błąd aplikacji ustawień".to_string()
        } else {
            messages.join(" | ")
        },
    }
}

#[cfg(windows)]
unsafe fn apply_gamma_ramp(settings: &VisualSettings) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
    use windows_sys::Win32::UI::ColorSystem::SetDeviceGammaRamp;

    let dc = GetDC(0 as _);
    if dc.is_null() {
        return false;
    }

    let mut ramp: [[u16; 256]; 3] = [[0; 256]; 3];
    let brightness = settings.brightness / 100.0;
    let contrast = settings.contrast / 100.0;

    for index in 0..256 {
        let input = index as f32 / 255.0;
        let adjusted = (input - 0.5) * contrast + 0.5 + brightness;
        let clamped = adjusted.clamp(0.0, 1.0);
        let value = (clamped * 65535.0).round() as u16;
        for channel in 0..3 {
            ramp[channel][index] = value;
        }
    }

    let result = SetDeviceGammaRamp(dc, ramp.as_ptr().cast());
    ReleaseDC(0 as _, dc);
    result != 0
}

#[cfg(windows)]
mod nvapi {
    use libloading::Library;
    use std::ffi::c_void;
    use std::sync::OnceLock;

    type NvApiStatus = i32;
    type NvDisplayHandle = *mut c_void;
    type QueryInterface = unsafe extern "C" fn(u32) -> *const c_void;
    type Initialize = unsafe extern "C" fn() -> NvApiStatus;
    type EnumNvidiaDisplayHandle = unsafe extern "C" fn(u32, *mut NvDisplayHandle) -> NvApiStatus;
    type GetDvcInfoEx =
        unsafe extern "C" fn(NvDisplayHandle, u32, *mut DisplayDvcInfo) -> NvApiStatus;
    type SetDvcLevelEx =
        unsafe extern "C" fn(NvDisplayHandle, u32, *mut DisplayDvcInfo) -> NvApiStatus;
    type SetDvcLevel = unsafe extern "C" fn(NvDisplayHandle, u32, i32) -> NvApiStatus;
    type SetHueAngle = unsafe extern "C" fn(NvDisplayHandle, u32, i32) -> NvApiStatus;

    const NVAPI_OK: NvApiStatus = 0;
    const QUERY_INITIALIZE: u32 = 0x0150E828;
    const QUERY_ENUM_NVIDIA_DISPLAY_HANDLE: u32 = 0x9ABDD40D;
    const QUERY_GET_DVC_INFO_EX: u32 = 0x0E45002D;
    const QUERY_SET_DVC_LEVEL_EX: u32 = 0x4A82C2B1;
    const QUERY_SET_DVC_LEVEL: u32 = 0x172409B4;
    const QUERY_SET_HUE_ANGLE: u32 = 0x0F5A0F22;

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct DisplayDvcInfo {
        pub version: u32,
        pub current_level: i32,
        pub min_level: i32,
        pub max_level: i32,
        pub default_level: i32,
    }

    static NVAPI_LIB: OnceLock<Option<Library>> = OnceLock::new();

    fn get_nvapi() -> Option<&'static Library> {
        NVAPI_LIB
            .get_or_init(|| unsafe { Library::new("nvapi64.dll").ok() })
            .as_ref()
    }

    pub unsafe fn try_apply_dvc(saturation: f32, hue: f32) -> Result<(bool, bool), String> {
        let lib = get_nvapi().ok_or("Brak nvapi64.dll")?;

        let nvapi_query_interface: libloading::Symbol<QueryInterface> = lib
            .get(b"nvapi_QueryInterface\0")
            .map_err(|e| e.to_string())?;

        let init_fn_ptr = nvapi_query_interface(QUERY_INITIALIZE);
        if init_fn_ptr.is_null() {
            return Err("Brak init".into());
        }
        let initialize: Initialize = std::mem::transmute(init_fn_ptr);
        if initialize() != NVAPI_OK {
            return Err("Init błąd".into());
        }

        let enum_fn_ptr = nvapi_query_interface(QUERY_ENUM_NVIDIA_DISPLAY_HANDLE);
        if enum_fn_ptr.is_null() {
            return Err("Brak enum".into());
        }
        let enum_display: EnumNvidiaDisplayHandle = std::mem::transmute(enum_fn_ptr);

        let mut display_handle: NvDisplayHandle = std::ptr::null_mut();
        if enum_display(0, &mut display_handle) != NVAPI_OK || display_handle.is_null() {
            return Err("Brak ekranów NV".into());
        }

        let mut dvc_applied = false;
        let get_dvc_ptr = nvapi_query_interface(QUERY_GET_DVC_INFO_EX);
        let set_dvc_ptr = nvapi_query_interface(QUERY_SET_DVC_LEVEL_EX);

        if !get_dvc_ptr.is_null() && !set_dvc_ptr.is_null() {
            let get_dvc_info: GetDvcInfoEx = std::mem::transmute(get_dvc_ptr);
            let set_dvc_level: SetDvcLevelEx = std::mem::transmute(set_dvc_ptr);

            let mut dvc_info: DisplayDvcInfo = std::mem::zeroed();
            dvc_info.version = std::mem::size_of::<DisplayDvcInfo>() as u32 | 0x10000;

            if get_dvc_info(display_handle, 0, &mut dvc_info) == NVAPI_OK {
                let pct = (saturation / 200.0).clamp(0.0, 1.0);
                let target_level = dvc_info.min_level
                    + ((dvc_info.max_level - dvc_info.min_level) as f32 * pct) as i32;
                dvc_info.current_level = target_level;

                if set_dvc_level(display_handle, 0, &mut dvc_info) == NVAPI_OK {
                    dvc_applied = true;
                } else {
                    let set_dvc_legacy_ptr = nvapi_query_interface(QUERY_SET_DVC_LEVEL);
                    if !set_dvc_legacy_ptr.is_null() {
                        let set_dvc_legacy: SetDvcLevel = std::mem::transmute(set_dvc_legacy_ptr);
                        dvc_applied = set_dvc_legacy(display_handle, 0, target_level) == NVAPI_OK;
                    }
                }
            }
        }

        let mut hue_applied = false;
        let set_hue_ptr = nvapi_query_interface(QUERY_SET_HUE_ANGLE);
        if !set_hue_ptr.is_null() {
            let set_hue_angle: SetHueAngle = std::mem::transmute(set_hue_ptr);
            let target_hue = hue.clamp(-180.0, 180.0).round() as i32;

            if set_hue_angle(display_handle, 0, target_hue) == NVAPI_OK {
                hue_applied = true;
            }
        }

        Ok((dvc_applied, hue_applied))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("GDK_BACKEND", "x11");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = check_for_update(handle).await {
                    eprintln!("Risxn Visuals updater: {error}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![runtime_info, apply_visuals])
        .run(tauri::generate_context!())
        .expect("error while running Risxn Visuals");
}

async fn check_for_update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    let update = app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .build()?
        .check()
        .await?;

    if let Some(update) = update {
        update
            .download_and_install(
                |chunk_length, content_length| {
                    eprintln!(
                        "Risxn Visuals updater: pobrano {chunk_length} bajtów, rozmiar {:?}",
                        content_length
                    );
                },
                || eprintln!("Risxn Visuals updater: pobieranie zakończone"),
            )
            .await?;
        app.restart();
    }

    Ok(())
}
