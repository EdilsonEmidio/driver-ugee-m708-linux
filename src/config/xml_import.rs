use crate::config::profiles::Action;
use crate::config::transform::Orientation;
use crate::config::DriverConfig;
use crate::error::{DriverError, DriverResult};
use std::fs;
use std::path::Path;

/// Importa configurações principais de um arquivo `Ugee_Tablet.xml` (ou `.ugeecfg` XML)
/// gerado pelo driver original do Windows para um `DriverConfig` Linux.
pub fn parse_ugee_xml(xml: &str) -> DriverResult<DriverConfig> {
    if xml.trim().is_empty() || !xml.contains('<') {
        return Err(DriverError::Config {
            path: Path::new("<xml>").to_path_buf(),
            reason: "conteúdo XML inválido ou vazio".to_string(),
        });
    }

    let mut cfg = DriverConfig::default();

    if let Some(ori) = find_tag_value(xml, "Orientation").and_then(|v| v.parse::<u8>().ok()) {
        cfg.orientation = match ori {
            1 => Orientation::Deg90,
            2 => Orientation::Deg180,
            3 => Orientation::Deg270,
            _ => Orientation::Deg0,
        };
    }

    if let Some(v) = find_tag_bool(xml, "DisablePressure") {
        cfg.disable_pressure = v;
    }
    if let Some(v) = find_tag_bool(xml, "DisableTilt") {
        cfg.disable_tilt = v;
    }
    if let Some(v) = find_tag_bool(xml, "DisableExpressKey") {
        cfg.disable_express_keys = v;
    }
    if let Some(v) = find_tag_bool(xml, "RelativeCoords") {
        cfg.relative_mode = v;
    }
    if let Some(v) = find_tag_value(xml, "MouseSpeed").and_then(|v| v.parse::<u8>().ok()) {
        cfg.mouse_speed = v.clamp(1, 9);
    }

    if let Some(tx) = find_tag_value(xml, "TX").and_then(|v| v.parse::<u32>().ok()) {
        cfg.active_area.x_hundredths_in = tx;
    }
    if let Some(ty) = find_tag_value(xml, "TY").and_then(|v| v.parse::<u32>().ok()) {
        cfg.active_area.y_hundredths_in = ty;
    }
    if let Some(tw) = find_tag_value(xml, "TW").and_then(|v| v.parse::<u32>().ok()) {
        cfg.active_area.w_hundredths_in = tw;
    }
    if let Some(th) = find_tag_value(xml, "TH").and_then(|v| v.parse::<u32>().ok()) {
        cfg.active_area.h_hundredths_in = th;
    }

    if let Some(p0) = find_tag_value(xml, "P0").and_then(|v| v.parse::<f64>().ok()) {
        cfg.pressure_curve.p0 = p0.clamp(0.0, 0.99);
    }
    if let Some(p1x) = find_tag_value(xml, "P1X").and_then(|v| v.parse::<f64>().ok()) {
        cfg.pressure_curve.p1x = p1x.clamp(0.0, 1.0);
    }
    if let Some(p1y) = find_tag_value(xml, "P1Y").and_then(|v| v.parse::<f64>().ok()) {
        cfg.pressure_curve.p1y = p1y.clamp(0.0, 1.0);
    }
    if let Some(p2) = find_tag_value(xml, "P2").and_then(|v| v.parse::<f64>().ok()) {
        cfg.pressure_curve.p2 = p2.clamp(0.01, 1.0);
    }

    if let Some(actid) = find_attr_u16(xml, "PenBtn1", "Actid") {
        let custom = find_attr_str(xml, "PenBtn1", "KeyStr");
        cfg.pen_buttons.barrel_1 = resolve_xml_action(actid, custom.as_deref());
    }
    if let Some(actid) = find_attr_u16(xml, "PenBtn2", "Actid") {
        let custom = find_attr_str(xml, "PenBtn2", "KeyStr");
        cfg.pen_buttons.barrel_2 = resolve_xml_action(actid, custom.as_deref());
    }

    for idx in 1..=40usize {
        let tag = format!("Key{idx}");
        if let Some(actid) = find_attr_u16(xml, &tag, "Actid") {
            let custom = find_attr_str(xml, &tag, "KeyStr");
            cfg.express_keys.insert(
                format!("k{idx}"),
                resolve_xml_action(actid, custom.as_deref()),
            );
        }
    }

    Ok(cfg)
}

fn resolve_xml_action(actid: u16, key_str: Option<&str>) -> Action {
    if actid == 11 {
        if let Some(combo) = key_str {
            if let Some(act) = Action::parse_key_combo_str(combo) {
                return act;
            }
        }
    }
    Action::from_actid(actid, None)
}

/// Lê e importa um arquivo `Ugee_Tablet.xml` a partir do caminho em disco.
pub fn import_ugee_xml_file(path: &Path) -> DriverResult<DriverConfig> {
    let raw = fs::read_to_string(path).map_err(|source| DriverError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_ugee_xml(&raw)
}

fn find_tag_bool(xml: &str, tag: &str) -> Option<bool> {
    let raw = find_tag_value(xml, tag)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

fn find_tag_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let rest = xml.get(start..)?;
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

fn find_attr_str(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let prefix = format!("<{tag}");
    let start = xml.find(&prefix)?;
    let rest = xml.get(start + prefix.len()..)?;
    let tag_end = rest.find('>')?;
    let attrs = &rest[..tag_end];
    let needle = format!("{attr}=\"");
    let attr_start = attrs.find(&needle)? + needle.len();
    let val_rest = attrs.get(attr_start..)?;
    let val_end = val_rest.find('"')?;
    Some(val_rest[..val_end].to_string())
}

fn find_attr_u16(xml: &str, tag: &str, attr: &str) -> Option<u16> {
    find_attr_str(xml, tag, attr)?.trim().parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::Key;

    #[test]
    fn imports_sample_ugee_tablet_xml() -> Result<(), Box<dyn std::error::Error>> {
        let sample = r#"
        <UgeeTablet>
            <Orientation>1</Orientation>
            <TX>100</TX><TY>50</TY><TW>800</TW><TH>500</TH>
            <P0>0.05</P0><P1X>0.40</P1X><P1Y>0.25</P1Y><P2>0.95</P2>
            <DisablePressure>1</DisablePressure>
            <RelativeCoords>1</RelativeCoords>
            <MouseSpeed>7</MouseSpeed>
            <PenBtn1 Actid="207" />
            <PenBtn2 Actid="1" />
            <Key1 Actid="306" />
            <Key2 Actid="11" KeyStr="Ctrl+Shift+Z" />
        </UgeeTablet>
        "#;

        let cfg = parse_ugee_xml(sample)?;
        assert_eq!(cfg.orientation, Orientation::Deg90);
        assert_eq!(cfg.active_area.x_hundredths_in, 100);
        assert_eq!(cfg.active_area.w_hundredths_in, 800);
        assert!(cfg.disable_pressure);
        assert!(cfg.relative_mode);
        assert_eq!(cfg.mouse_speed, 7);
        assert_eq!(cfg.pen_buttons.barrel_1, Action::MouseRight);
        assert_eq!(cfg.pen_buttons.barrel_2, Action::ToggleEraser);
        assert_eq!(
            cfg.express_keys.get("k1"),
            Some(&Action::combo(&[Key::KEY_LEFTCTRL, Key::KEY_Z]))
        );
        assert_eq!(
            cfg.express_keys.get("k2"),
            Some(&Action::combo(&[
                Key::KEY_LEFTCTRL,
                Key::KEY_LEFTSHIFT,
                Key::KEY_Z
            ]))
        );
        Ok(())
    }
}
