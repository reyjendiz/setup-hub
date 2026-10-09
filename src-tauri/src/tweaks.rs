use crate::util;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use winreg::enums::*;
use winreg::RegKey;

pub const HIGH_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
/// Control Panel pointer-speed slider: 11 notches → MouseSensitivity values.
pub const NOTCHES: [u32; 11] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20];
pub const TARGET_NOTCH: usize = 5;

#[derive(Serialize)]
pub struct TweakState {
    pub id: &'static str,
    pub applied: bool,
    pub can_revert: bool,
    pub detail: String,
}

pub const IDS: [&str; 6] = ["mouse_speed", "mouse_precision", "power_plan", "print_screen", "hibernate", "fast_startup"];

fn state_path() -> std::path::PathBuf {
    util::app_dir().join("tweaks.json")
}

fn saved() -> Value {
    std::fs::read_to_string(state_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}))
}

/// Remembers the pre-tweak values once; re-applying never overwrites the original snapshot.
fn remember(id: &str, v: Value) -> Result<()> {
    let mut s = saved();
    if s.get(id).is_none() {
        s[id] = v;
        std::fs::write(state_path(), serde_json::to_string_pretty(&s)?)?;
    }
    Ok(())
}

fn forget(id: &str) -> Result<()> {
    let mut s = saved();
    if let Some(o) = s.as_object_mut() {
        o.remove(id);
    }
    std::fs::write(state_path(), serde_json::to_string_pretty(&s)?)?;
    Ok(())
}

// ---------- mouse ----------

fn mouse_key() -> Result<RegKey> {
    Ok(RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(r"Control Panel\Mouse", KEY_READ | KEY_WRITE)?)
}

fn reg_str(k: &RegKey, name: &str) -> String {
    k.get_value::<String, _>(name).unwrap_or_default()
}

fn spi_speed(v: u32) -> Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    // SAFETY: SPI_SETMOUSESPEED takes the speed as the pvParam pointer value itself.
    let ok = unsafe { SystemParametersInfoW(SPI_SETMOUSESPEED, 0, v as usize as *mut _, SPIF_UPDATEINIFILE | SPIF_SENDWININICHANGE) };
    if ok == 0 { bail!("SystemParametersInfo(SPI_SETMOUSESPEED) failed") }
    Ok(())
}

fn spi_mouse(t1: i32, t2: i32, accel: i32) -> Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut a = [t1, t2, accel];
    // SAFETY: SPI_SETMOUSE expects a pointer to an array of three ints.
    let ok = unsafe { SystemParametersInfoW(SPI_SETMOUSE, 0, a.as_mut_ptr().cast(), SPIF_UPDATEINIFILE | SPIF_SENDWININICHANGE) };
    if ok == 0 { bail!("SystemParametersInfo(SPI_SETMOUSE) failed") }
    Ok(())
}

pub fn precision_on(speed: &str, t1: &str, t2: &str) -> bool {
    !(speed == "0" && t1 == "0" && t2 == "0")
}

fn mouse_state(id: &'static str) -> Result<TweakState> {
    let k = mouse_key()?;
    let can_revert = saved().get(id).is_some();
    Ok(match id {
        "mouse_speed" => {
            let cur = reg_str(&k, "MouseSensitivity");
            let notch = cur.parse::<u32>().ok().and_then(|v| NOTCHES.iter().position(|n| *n == v)).map(|i| i + 1);
            TweakState {
                id,
                applied: cur == NOTCHES[TARGET_NOTCH - 1].to_string(),
                can_revert,
                detail: notch.map(|n| format!("{n}/11")).unwrap_or(cur),
            }
        }
        _ => {
            let on = precision_on(&reg_str(&k, "MouseSpeed"), &reg_str(&k, "MouseThreshold1"), &reg_str(&k, "MouseThreshold2"));
            TweakState { id, applied: !on, can_revert, detail: if on { "on".into() } else { "off".into() } }
        }
    })
}

fn mouse_apply(id: &str) -> Result<()> {
    let k = mouse_key()?;
    if id == "mouse_speed" {
        remember(id, json!({ "MouseSensitivity": reg_str(&k, "MouseSensitivity") }))?;
        let v = NOTCHES[TARGET_NOTCH - 1];
        k.set_value("MouseSensitivity", &v.to_string())?;
        spi_speed(v)?;
        tracing::info!("tweak mouse_speed: MouseSensitivity -> {v}");
    } else {
        remember(id, json!({
            "MouseSpeed": reg_str(&k, "MouseSpeed"),
            "MouseThreshold1": reg_str(&k, "MouseThreshold1"),
            "MouseThreshold2": reg_str(&k, "MouseThreshold2"),
        }))?;
        for n in ["MouseSpeed", "MouseThreshold1", "MouseThreshold2"] {
            k.set_value(n, &"0")?;
        }
        spi_mouse(0, 0, 0)?;
        tracing::info!("tweak mouse_precision: MouseSpeed/Threshold1/Threshold2 -> 0");
    }
    Ok(())
}

fn mouse_revert(id: &str) -> Result<()> {
    let s = saved();
    let old = s.get(id).context("nothing to revert")?;
    let k = mouse_key()?;
    for (name, v) in old.as_object().unwrap() {
        k.set_value(name, &v.as_str().unwrap_or("").to_string())?;
    }
    if id == "mouse_speed" {
        spi_speed(old["MouseSensitivity"].as_str().unwrap_or("10").parse().unwrap_or(10))?;
    } else {
        let p = |n: &str| old[n].as_str().unwrap_or("0").parse::<i32>().unwrap_or(0);
        spi_mouse(p("MouseThreshold1"), p("MouseThreshold2"), p("MouseSpeed"))?;
    }
    tracing::info!("tweak {id}: reverted to {old}");
    forget(id)
}

// ---------- power ----------

async fn powercfg(args: &[&str]) -> Result<String> {
    let (c, o) = util::run("powercfg.exe", args).await?;
    if c != 0 {
        bail!("powercfg {} failed: {}", args.join(" "), o.trim());
    }
    Ok(o)
}

/// First GUID in powercfg output (locale independent).
pub fn parse_guid(s: &str) -> Option<String> {
    regex::Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
        .unwrap()
        .find(s)
        .map(|m| m.as_str().to_lowercase())
}

/// AC and DC values from `powercfg /query`: the last two hex numbers, regardless of UI language.
pub fn parse_ac_dc(s: &str) -> Option<(u32, u32)> {
    let re = regex::Regex::new(r":\s*0x([0-9a-fA-F]{8})\s*$").unwrap();
    let v: Vec<u32> = s.lines().filter_map(|l| re.captures(l.trim_end())).filter_map(|c| u32::from_str_radix(&c[1], 16).ok()).collect();
    (v.len() >= 2).then(|| (v[v.len() - 2], v[v.len() - 1]))
}

const TIMEOUTS: [(&str, &str, &str); 3] = [
    ("SUB_VIDEO", "VIDEOIDLE", "monitor-timeout"),
    ("SUB_SLEEP", "STANDBYIDLE", "standby-timeout"),
    ("SUB_SLEEP", "HIBERNATEIDLE", "hibernate-timeout"),
];

async fn timeouts() -> Result<Vec<(u32, u32)>> {
    let mut v = Vec::new();
    for (sub, set, _) in TIMEOUTS {
        let o = powercfg(&["/query", "SCHEME_CURRENT", sub, set]).await?;
        v.push(parse_ac_dc(&o).with_context(|| format!("could not parse powercfg {set}"))?);
    }
    Ok(v)
}

async fn set_timeouts(v: &[(u32, u32)]) -> Result<()> {
    for ((_, _, name), (ac, dc)) in TIMEOUTS.iter().zip(v) {
        // powercfg /change takes minutes; /query reports seconds.
        powercfg(&["/change", &format!("{name}-ac"), &(ac / 60).to_string()]).await?;
        powercfg(&["/change", &format!("{name}-dc"), &(dc / 60).to_string()]).await?;
    }
    Ok(())
}

async fn power_state() -> Result<TweakState> {
    let active = parse_guid(&powercfg(&["/getactivescheme"]).await?).unwrap_or_default();
    let t = timeouts().await?;
    let name = powercfg(&["/getactivescheme"]).await?;
    let name = name.split('(').nth(1).and_then(|s| s.split(')').next()).unwrap_or("").to_string();
    Ok(TweakState {
        id: "power_plan",
        applied: active == HIGH_PERF && t.iter().all(|(a, d)| *a == 0 && *d == 0),
        can_revert: saved().get("power_plan").is_some(),
        detail: name,
    })
}

async fn power_apply() -> Result<()> {
    let prev = parse_guid(&powercfg(&["/getactivescheme"]).await?).context("no active scheme")?;
    let list = powercfg(&["/list"]).await?;
    if !list.to_lowercase().contains(HIGH_PERF) {
        tracing::info!("High performance plan missing, duplicating it");
        powercfg(&["-duplicatescheme", HIGH_PERF, HIGH_PERF]).await?;
    }
    powercfg(&["/setactive", HIGH_PERF]).await?;
    let before = timeouts().await?;
    remember("power_plan", json!({ "scheme": prev, "timeouts": before }))?;
    set_timeouts(&[(0, 0), (0, 0), (0, 0)]).await?;
    tracing::info!("tweak power_plan: {prev} -> {HIGH_PERF}, timeouts {before:?} -> all 0");
    Ok(())
}

async fn power_revert() -> Result<()> {
    let s = saved();
    let old = s.get("power_plan").context("nothing to revert")?;
    let t: Vec<(u32, u32)> = serde_json::from_value(old["timeouts"].clone())?;
    powercfg(&["/setactive", HIGH_PERF]).await?;
    set_timeouts(&t).await?;
    powercfg(&["/setactive", old["scheme"].as_str().unwrap_or(HIGH_PERF)]).await?;
    tracing::info!("tweak power_plan: reverted to {old}");
    forget("power_plan")
}

// ---------- Print Screen ----------

const KEYBOARD_KEY: &str = r"Control Panel\Keyboard";
const PRTSC_VALUE: &str = "PrintScreenKeyForSnippingEnabled";

fn print_screen_value() -> Option<u32> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEYBOARD_KEY).and_then(|k| k.get_value::<u32, _>(PRTSC_VALUE)).ok()
}

/// Whether Print Screen opens Snipping Tool. Unset means on for Windows 11, off for Windows 10.
pub fn snipping_on_print_screen(value: Option<u32>, build: u32) -> bool {
    value.map_or(build >= 22000, |v| v != 0)
}

/// Frees the key for a screenshot app (Flameshot registers Print Screen itself, which fails while
/// Windows hands the key to Snipping Tool).
fn print_screen_apply() -> Result<()> {
    let prev = print_screen_value();
    remember("print_screen", json!({ "value": prev }))?;
    RegKey::predef(HKEY_CURRENT_USER).create_subkey(KEYBOARD_KEY)?.0.set_value(PRTSC_VALUE, &0u32)?;
    tracing::info!("tweak print_screen: {PRTSC_VALUE} {prev:?} -> 0");
    Ok(())
}

fn print_screen_revert() -> Result<()> {
    let s = saved();
    let old = s.get("print_screen").context("nothing to revert")?;
    let k = RegKey::predef(HKEY_CURRENT_USER).create_subkey(KEYBOARD_KEY)?.0;
    match old["value"].as_u64() {
        Some(v) => k.set_value(PRTSC_VALUE, &(v as u32))?,
        None => {
            let _ = k.delete_value(PRTSC_VALUE);
        }
    }
    tracing::info!("tweak print_screen: reverted to {old}");
    forget("print_screen")
}

// ---------- optional toggles ----------

fn hiberboot_key() -> Result<RegKey> {
    Ok(RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager\Power", KEY_READ | KEY_WRITE)?)
}

fn hibernation_enabled() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Power")
        .and_then(|k| k.get_value::<u32, _>("HibernateEnabled"))
        .map(|v| v != 0)
        .unwrap_or(false)
}

pub async fn state(id: &str) -> Result<TweakState> {
    match id {
        "mouse_speed" => mouse_state("mouse_speed"),
        "mouse_precision" => mouse_state("mouse_precision"),
        "power_plan" => power_state().await,
        "print_screen" => {
            let snipping = snipping_on_print_screen(print_screen_value(), crate::gpu::windows_build().await);
            Ok(TweakState {
                id: "print_screen",
                applied: !snipping,
                can_revert: saved().get("print_screen").is_some(),
                detail: if snipping { "on".into() } else { "off".into() },
            })
        }
        "hibernate" => {
            let on = hibernation_enabled();
            Ok(TweakState { id: "hibernate", applied: !on, can_revert: !on, detail: if on { "on".into() } else { "off".into() } })
        }
        "fast_startup" => {
            let on = hiberboot_key().and_then(|k| Ok(k.get_value::<u32, _>("HiberbootEnabled")?)).unwrap_or(1) != 0;
            Ok(TweakState { id: "fast_startup", applied: !on, can_revert: !on, detail: if on { "on".into() } else { "off".into() } })
        }
        _ => bail!("unknown tweak {id}"),
    }
}

pub async fn apply(id: &str) -> Result<()> {
    match id {
        "mouse_speed" | "mouse_precision" => mouse_apply(id),
        "power_plan" => power_apply().await,
        "print_screen" => print_screen_apply(),
        "hibernate" => powercfg(&["/hibernate", "off"]).await.map(|_| tracing::info!("tweak hibernate: off")),
        "fast_startup" => {
            hiberboot_key()?.set_value("HiberbootEnabled", &0u32)?;
            tracing::info!("tweak fast_startup: HiberbootEnabled -> 0");
            Ok(())
        }
        _ => bail!("unknown tweak {id}"),
    }
}

pub async fn revert(id: &str) -> Result<()> {
    match id {
        "mouse_speed" | "mouse_precision" => mouse_revert(id),
        "power_plan" => power_revert().await,
        "print_screen" => print_screen_revert(),
        "hibernate" => powercfg(&["/hibernate", "on"]).await.map(|_| tracing::info!("tweak hibernate: on")),
        "fast_startup" => {
            hiberboot_key()?.set_value("HiberbootEnabled", &1u32)?;
            tracing::info!("tweak fast_startup: HiberbootEnabled -> 1");
            Ok(())
        }
        _ => bail!("unknown tweak {id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifth_notch_is_eight() {
        assert_eq!(NOTCHES[TARGET_NOTCH - 1], 8);
        assert_eq!(NOTCHES[5], 10, "Windows default is the 6th notch");
    }

    #[test]
    fn print_screen_default_depends_on_windows_version() {
        assert!(snipping_on_print_screen(None, 26100));
        assert!(!snipping_on_print_screen(None, 19045));
        assert!(!snipping_on_print_screen(Some(0), 26100));
        assert!(snipping_on_print_screen(Some(1), 19045));
    }

    #[test]
    fn precision_detection() {
        assert!(precision_on("1", "6", "10"));
        assert!(!precision_on("0", "0", "0"));
        assert!(precision_on("0", "6", "0"));
    }

    #[test]
    fn powercfg_parsing_is_locale_independent() {
        let en = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)";
        assert_eq!(parse_guid(en).unwrap(), "381b4222-f694-41f0-9685-ff5bb260df2e");
        let ru = "GUID схемы питания: 8C5E7FDA-E8BF-4A96-9A85-A6E23A8C635C  (Высокая производительность)";
        assert_eq!(parse_guid(ru).unwrap(), HIGH_PERF);

        let q = "Power Setting GUID: 3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e  (Turn off display after)\n  Minimum Possible Setting: 0x00000000\n  Maximum Possible Setting: 0xffffffff\n  Possible Settings increment: 0x00000001\n  Possible Settings units: Seconds\nCurrent AC Power Setting Index: 0x0000012c\nCurrent DC Power Setting Index: 0x000000b4\n";
        assert_eq!(parse_ac_dc(q).unwrap(), (300, 180));
        let qru = "Индекс текущей настройки питания от сети: 0x00000000\nИндекс текущей настройки питания от батареи: 0x00000708\n";
        assert_eq!(parse_ac_dc(qru).unwrap(), (0, 1800));
        assert!(parse_ac_dc("garbage").is_none());
    }
}
