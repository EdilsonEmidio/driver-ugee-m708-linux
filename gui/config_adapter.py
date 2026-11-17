"""
Adaptador bidirecional entre o dicionário de configuração/UI da interface GTK4
e o schema JSON de `DriverConfig` / `DaemonSnapshot` do daemon Rust (`ugee-driver`).
"""

import copy

# Códigos Linux `input-event-codes.h` correspondentes a `evdev::Key` no Rust.
KEY_NAME_TO_CODE = {
    "KEY_ESC": 1,
    "KEY_1": 2,
    "KEY_2": 3,
    "KEY_3": 4,
    "KEY_4": 5,
    "KEY_5": 6,
    "KEY_6": 7,
    "KEY_7": 8,
    "KEY_8": 9,
    "KEY_9": 10,
    "KEY_0": 11,
    "KEY_MINUS": 12,
    "KEY_EQUAL": 13,
    "KEY_BACKSPACE": 14,
    "KEY_TAB": 15,
    "KEY_Q": 16,
    "KEY_W": 17,
    "KEY_E": 18,
    "KEY_R": 19,
    "KEY_T": 20,
    "KEY_Y": 21,
    "KEY_U": 22,
    "KEY_I": 23,
    "KEY_O": 24,
    "KEY_P": 25,
    "KEY_LEFTBRACE": 26,
    "KEY_RIGHTBRACE": 27,
    "KEY_ENTER": 28,
    "KEY_LEFTCTRL": 29,
    "KEY_A": 30,
    "KEY_S": 31,
    "KEY_D": 32,
    "KEY_F": 33,
    "KEY_G": 34,
    "KEY_H": 35,
    "KEY_J": 36,
    "KEY_K": 37,
    "KEY_L": 38,
    "KEY_LEFTSHIFT": 42,
    "KEY_Z": 44,
    "KEY_X": 45,
    "KEY_C": 46,
    "KEY_V": 47,
    "KEY_B": 48,
    "KEY_N": 49,
    "KEY_M": 50,
    "KEY_RIGHTSHIFT": 54,
    "KEY_LEFTALT": 56,
    "KEY_SPACE": 57,
    "KEY_F1": 59,
    "KEY_F2": 60,
    "KEY_F3": 61,
    "KEY_F4": 62,
    "KEY_F5": 63,
    "KEY_F6": 64,
    "KEY_F7": 65,
    "KEY_F8": 66,
    "KEY_F9": 67,
    "KEY_F10": 68,
    "KEY_F11": 87,
    "KEY_F12": 88,
    "KEY_RIGHTCTRL": 97,
    "KEY_RIGHTALT": 100,
    "KEY_HOME": 102,
    "KEY_UP": 103,
    "KEY_PAGEUP": 104,
    "KEY_LEFT": 105,
    "KEY_RIGHT": 106,
    "KEY_END": 107,
    "KEY_DOWN": 108,
    "KEY_PAGEDOWN": 109,
    "KEY_DELETE": 111,
    "KEY_MUTE": 113,
    "KEY_VOLUMEDOWN": 114,
    "KEY_VOLUMEUP": 115,
    "KEY_LEFTMETA": 125,
    "KEY_NEXTSONG": 163,
    "KEY_PLAYPAUSE": 164,
    "KEY_PREVIOUSSONG": 165,
    "BTN_LEFT": 272,
    "BTN_RIGHT": 273,
    "BTN_MIDDLE": 274,
}

CODE_TO_KEY_NAME = {code: name for name, code in KEY_NAME_TO_CODE.items()}

KEY_ALIASES = {
    "CTRL": "KEY_LEFTCTRL",
    "CONTROL": "KEY_LEFTCTRL",
    "LCTRL": "KEY_LEFTCTRL",
    "RCTRL": "KEY_RIGHTCTRL",
    "SHIFT": "KEY_LEFTSHIFT",
    "LSHIFT": "KEY_LEFTSHIFT",
    "RSHIFT": "KEY_RIGHTSHIFT",
    "ALT": "KEY_LEFTALT",
    "LALT": "KEY_LEFTALT",
    "RALT": "KEY_RIGHTALT",
    "ALTGR": "KEY_RIGHTALT",
    "SUPER": "KEY_LEFTMETA",
    "META": "KEY_LEFTMETA",
    "WIN": "KEY_LEFTMETA",
    "ESPACO": "KEY_SPACE",
    "DEL": "KEY_DELETE",
    "ESCAPE": "KEY_ESC",
    "RETURN": "KEY_ENTER",
    "PLUS": "KEY_EQUAL",
    "+": "KEY_EQUAL",
    "=": "KEY_EQUAL",
    "-": "KEY_MINUS",
    "[": "KEY_LEFTBRACE",
    "]": "KEY_RIGHTBRACE",
}

ORIENTATION_TO_DEG = {
    "deg0": 0,
    "deg90": 90,
    "deg180": 180,
    "deg270": 270,
}
DEG_TO_ORIENTATION = {deg: name for name, deg in ORIENTATION_TO_DEG.items()}


def parse_combo_to_codes(combo_str: str) -> list[int]:
    """Converte string de atalho (ex.: `KEY_LEFTCTRL+KEY_Z` ou `Ctrl+Z`) em códigos `evdev`."""
    if not combo_str:
        return []
    codes: list[int] = []
    for raw_part in combo_str.split("+"):
        part = raw_part.strip().upper()
        if not part:
            continue
        if part in KEY_ALIASES:
            part = KEY_ALIASES[part]
        elif not part.startswith("KEY_") and not part.startswith("BTN_"):
            part = f"KEY_{part}"
        code = KEY_NAME_TO_CODE.get(part)
        if code is not None:
            codes.append(code)
    return codes


def codes_to_combo_str(codes: list[int]) -> str:
    """Converte lista de códigos `evdev` numéricos para representação canônica `KEY_*+KEY_*`."""
    names = []
    for code in codes:
        name = CODE_TO_KEY_NAME.get(int(code))
        if name:
            names.append(name)
        else:
            names.append(f"KEY_{int(code)}")
    return "+".join(names)


def get_default_gui_config() -> dict:
    """Retorna a estrutura de configuração inicial utilizada pelos widgets da GUI."""
    return {
        "workarea": {
            "rotation": 0,
            "keep_aspect_ratio": False,
            "precision_mode": False,
            "tablet_area": {"x_min": 0.0, "y_min": 0.0, "x_max": 1.0, "y_max": 1.0},
            "screen_area": {"x_min": 0.0, "y_min": 0.0, "x_max": 1.0, "y_max": 1.0},
        },
        "pen": {
            "pressure_enabled": True,
            "tilt_enabled": True,
            "pressure_curve": {
                "p0": 0.0,
                "p1x": 0.5,
                "p1y": 0.5,
                "p2": 1.0,
                "points": [{"x": 0.5, "y": 0.5}],
            },
            "btn1_action": "BTN_STYLUS",
            "btn2_action": "ERASER",
        },
        "express_keys": {
            "enabled": True,
            "osd_notifications": True,
            "keys": [
                {"id": "K1", "label": "Pincel (B)",            "type": "shortcut",   "value": "KEY_B"},
                {"id": "K2", "label": "Borracha (E)",          "type": "shortcut",   "value": "KEY_E"},
                {"id": "K3", "label": "Modificador Ctrl",      "type": "shortcut",   "value": "KEY_LEFTCTRL"},
                {"id": "K4", "label": "Mão / Pan (Espaço)",    "type": "shortcut",   "value": "KEY_SPACE"},
                {"id": "K5", "label": "Desfazer (Ctrl+Z)",     "type": "shortcut",   "value": "KEY_LEFTCTRL+KEY_Z"},
                {"id": "K6", "label": "Zoom + (Ctrl++)",       "type": "shortcut",   "value": "KEY_LEFTCTRL+KEY_EQUAL"},
                {"id": "K7", "label": "Zoom - (Ctrl+-)",       "type": "shortcut",   "value": "KEY_LEFTCTRL+KEY_MINUS"},
                {"id": "K8", "label": "Salvar (Ctrl+S)",       "type": "shortcut",   "value": "KEY_LEFTCTRL+KEY_S"},
            ],
        },
    }


def rust_action_to_pen_btn(action: dict) -> str:
    """Converte `Action` JSON do Rust para o valor usado em `PEN_BTN_OPTIONS`."""
    if not isinstance(action, dict):
        return "DISABLED"
    atype = action.get("type", "none")
    if atype == "mouse_right":
        return "BTN_STYLUS"
    if atype == "mouse_middle":
        return "BTN_STYLUS2"
    if atype == "mouse_left":
        return "MOUSE_LEFT"
    if atype == "toggle_eraser":
        return "ERASER"
    if atype == "toggle_brush_eraser_keys":
        return "TOGGLE_BE"
    if atype == "key_combo":
        combo = codes_to_combo_str(action.get("keys", []))
        return combo or "DISABLED"
    return "DISABLED"


def pen_btn_to_rust_action(btn_val: str) -> dict:
    """Converte o código selecionado em `PEN_BTN_OPTIONS` para `Action` JSON do Rust."""
    if btn_val == "BTN_STYLUS":
        return {"type": "mouse_right"}
    if btn_val == "BTN_STYLUS2":
        return {"type": "mouse_middle"}
    if btn_val == "MOUSE_LEFT":
        return {"type": "mouse_left"}
    if btn_val == "ERASER":
        return {"type": "toggle_eraser"}
    if btn_val == "TOGGLE_BE":
        return {"type": "toggle_brush_eraser_keys"}
    if btn_val == "DISABLED":
        return {"type": "none"}
    codes = parse_combo_to_codes(btn_val)
    if codes:
        return {"type": "key_combo", "keys": codes}
    return {"type": "none"}


def rust_action_to_gui_key(action: dict, idx: int, custom_label: str | None = None) -> dict:
    """Converte `Action` JSON do Rust para item de tecla `K{idx+1}` da GUI."""
    kid = f"K{idx + 1}"
    atype = action.get("type", "none") if isinstance(action, dict) else "none"

    if atype == "key_combo":
        val = codes_to_combo_str(action.get("keys", []))
        return {"id": kid, "label": custom_label or val or kid, "type": "shortcut", "value": val}
    if atype == "mouse_wheel":
        delta = int(action.get("delta", 1))
        val = "SCROLL_UP" if delta >= 0 else "SCROLL_DOWN"
        default_lbl = "Rolagem Acima (Scroll+)" if delta >= 0 else "Rolagem Abaixo (Scroll-)"
        return {"id": kid, "label": custom_label or default_lbl, "type": "shortcut", "value": val}
    if atype == "toggle_precision_mode":
        return {"id": kid, "label": custom_label or "Alternar Modo Precisão", "type": "precision_toggle", "value": ""}
    if atype == "toggle_eraser":
        return {"id": kid, "label": custom_label or "Alternar Caneta/Borracha", "type": "eraser_toggle", "value": ""}
    if atype == "toggle_brush_eraser_keys":
        return {"id": kid, "label": custom_label or "Alternar [B] / [E]", "type": "toggle_brush_eraser", "value": ""}
    if atype == "mouse_left":
        return {"id": kid, "label": custom_label or "Mouse: Clique Esquerdo", "type": "mouse_left", "value": ""}
    if atype == "mouse_right":
        return {"id": kid, "label": custom_label or "Mouse: Clique Direito", "type": "mouse_right", "value": ""}
    if atype == "mouse_middle":
        return {"id": kid, "label": custom_label or "Mouse: Clique do Meio", "type": "mouse_middle", "value": ""}
    if atype == "mouse_double_left":
        return {"id": kid, "label": custom_label or "Mouse: Duplo Clique", "type": "mouse_double_left", "value": ""}
    if atype == "open_gui":
        return {"id": kid, "label": custom_label or "Abrir Painel Ugee", "type": "open_gui", "value": "ugee-gui"}
    if atype == "tablet_button":
        return {"id": kid, "label": custom_label or f"Botão Nativo ({kid})", "type": "tablet_button", "value": ""}
    if atype == "run_command":
        cmd = str(action.get("command", ""))
        return {"id": kid, "label": custom_label or (cmd or "Executar Aplicativo"), "type": "run_cmd", "value": cmd}
    return {"id": kid, "label": custom_label or "Sem Função (Desativado)", "type": "disabled", "value": ""}


def gui_key_to_rust_action(k_obj: dict, idx: int) -> dict:
    """Converte item de tecla da GUI (`{id, label, type, value}`) para `Action` JSON do Rust."""
    k_type = k_obj.get("type", "disabled")
    k_val = str(k_obj.get("value", "")).strip()

    if k_type == "shortcut":
        if k_val == "SCROLL_UP":
            return {"type": "mouse_wheel", "delta": 1}
        if k_val == "SCROLL_DOWN":
            return {"type": "mouse_wheel", "delta": -1}
        codes = parse_combo_to_codes(k_val)
        return {"type": "key_combo", "keys": codes} if codes else {"type": "none"}
    if k_type == "precision_toggle":
        return {"type": "toggle_precision_mode"}
    if k_type == "eraser_toggle":
        return {"type": "toggle_eraser"}
    if k_type == "toggle_brush_eraser":
        return {"type": "toggle_brush_eraser_keys"}
    if k_type == "mouse_left":
        return {"type": "mouse_left"}
    if k_type == "mouse_right":
        return {"type": "mouse_right"}
    if k_type == "mouse_middle":
        return {"type": "mouse_middle"}
    if k_type == "mouse_double_left":
        return {"type": "mouse_double_left"}
    if k_type == "open_gui":
        return {"type": "open_gui"}
    if k_type == "tablet_button":
        return {"type": "tablet_button", "index": min(max(int(idx), 0), 7)}
    if k_type == "run_cmd":
        return {"type": "run_command", "command": k_val}
    return {"type": "none"}


def _effective_hin_bounds(max_x: int, max_y: int, lpi: int, rotation: int) -> tuple[float, float]:
    lpi_safe = max(1, int(lpi))
    if int(rotation) % 180 == 90:
        eff_w, eff_h = max(1, int(max_y)), max(1, int(max_x))
    else:
        eff_w, eff_h = max(1, int(max_x)), max(1, int(max_y))
    return (eff_w * 100.0) / lpi_safe, (eff_h * 100.0) / lpi_safe


def rust_config_to_gui_config(
    rust_cfg: dict,
    max_x: int = 50794,
    max_y: int = 30474,
    lpi: int = 5080,
    key_count: int = 8,
) -> dict:
    """Traduz `DriverConfig` serializado em JSON pelo Rust para o formato interno da GUI."""
    gui = get_default_gui_config()
    if not isinstance(rust_cfg, dict):
        return gui

    rot = ORIENTATION_TO_DEG.get(str(rust_cfg.get("orientation", "deg0")).lower(), 0)
    gui["workarea"]["rotation"] = rot
    gui["workarea"]["precision_mode"] = bool(rust_cfg.get("precision_mode", False))

    full_w_hin, full_h_hin = _effective_hin_bounds(max_x, max_y, lpi, rot)
    aa = rust_cfg.get("active_area", {})
    aw = float(aa.get("w_hundredths_in", 0))
    ah = float(aa.get("h_hundredths_in", 0))
    if aw > 0.0 and ah > 0.0:
        ax = float(aa.get("x_hundredths_in", 0))
        ay = float(aa.get("y_hundredths_in", 0))
        xmin = round(max(0.0, min(0.90, ax / full_w_hin)), 3)
        ymin = round(max(0.0, min(0.90, ay / full_h_hin)), 3)
        xmax = round(max(xmin + 0.05, min(1.0, (ax + aw) / full_w_hin)), 3)
        ymax = round(max(ymin + 0.05, min(1.0, (ay + ah) / full_h_hin)), 3)
        gui["workarea"]["tablet_area"] = {
            "x_min": xmin,
            "y_min": ymin,
            "x_max": xmax,
            "y_max": ymax,
        }

    sm = rust_cfg.get("screen_mapping", {})
    sw = float(sm.get("sw", 0))
    sh = float(sm.get("sh", 0))
    fw = float(sm.get("full_w", 0))
    fh = float(sm.get("full_h", 0))
    if sw > 0.0 and sh > 0.0 and fw > 0.0 and fh > 0.0:
        sx = float(sm.get("sx", 0))
        sy = float(sm.get("sy", 0))
        sxmin = round(max(0.0, min(0.90, sx / fw)), 3)
        symin = round(max(0.0, min(0.90, sy / fh)), 3)
        sxmax = round(max(sxmin + 0.05, min(1.0, (sx + sw) / fw)), 3)
        symax = round(max(symin + 0.05, min(1.0, (sy + sh) / fh)), 3)
        gui["workarea"]["screen_area"] = {
            "x_min": sxmin,
            "y_min": symin,
            "x_max": sxmax,
            "y_max": symax,
        }

    pc = rust_cfg.get("pressure_curve", {})
    p1x = float(pc.get("p1x", 0.5))
    p1y = float(pc.get("p1y", 0.5))
    raw_pts = pc.get("points", [])
    pts = []
    if isinstance(raw_pts, list):
        for pt in raw_pts:
            if isinstance(pt, dict) and "x" in pt and "y" in pt:
                pts.append({"x": float(pt["x"]), "y": float(pt["y"])})
    if not pts:
        pts = [{"x": p1x, "y": p1y}]
    pts.sort(key=lambda pt: pt["x"])

    gui["pen"]["pressure_enabled"] = not bool(rust_cfg.get("disable_pressure", False))
    gui["pen"]["tilt_enabled"] = not bool(rust_cfg.get("disable_tilt", False))
    gui["pen"]["pressure_curve"] = {
        "p0": float(pc.get("p0", 0.0)),
        "p1x": pts[0]["x"],
        "p1y": pts[0]["y"],
        "p2": float(pc.get("p2", 1.0)),
        "points": pts,
    }

    pb = rust_cfg.get("pen_buttons", {})
    gui["pen"]["btn1_action"] = rust_action_to_pen_btn(pb.get("barrel_1", {}))
    gui["pen"]["btn2_action"] = rust_action_to_pen_btn(pb.get("barrel_2", {}))

    gui["express_keys"]["enabled"] = not bool(rust_cfg.get("disable_express_keys", False))
    gui["express_keys"]["osd_notifications"] = bool(rust_cfg.get("osd_notifications", True))

    rust_ek = rust_cfg.get("express_keys", {})
    rust_labels = rust_cfg.get("express_key_labels", {})
    total_keys = max(8, int(key_count or 8))
    keys_list = []
    for i in range(total_keys):
        k_name = f"k{i + 1}"
        act = rust_ek.get(k_name, {"type": "none"})
        lbl = rust_labels.get(k_name)
        keys_list.append(rust_action_to_gui_key(act, i, lbl))
    gui["express_keys"]["keys"] = keys_list
    return gui


def gui_config_to_rust_config(
    gui_cfg: dict,
    base_rust_cfg: dict | None = None,
    max_x: int = 50794,
    max_y: int = 30474,
    lpi: int = 5080,
) -> dict:
    """Traduz o dicionário da GUI GTK4 para `DriverConfig` JSON aceito por `UpdateConfig` no Rust."""
    out = copy.deepcopy(base_rust_cfg) if isinstance(base_rust_cfg, dict) else {}

    wa = gui_cfg.get("workarea", {})
    rot = int(wa.get("rotation", 0)) % 360
    out["orientation"] = DEG_TO_ORIENTATION.get(rot, "deg0")
    out["precision_mode"] = bool(wa.get("precision_mode", False))

    ta = wa.get("tablet_area", {})
    xmin = float(ta.get("x_min", 0.0))
    ymin = float(ta.get("y_min", 0.0))
    xmax = float(ta.get("x_max", 1.0))
    ymax = float(ta.get("y_max", 1.0))
    if xmin <= 0.001 and ymin <= 0.001 and xmax >= 0.999 and ymax >= 0.999:
        out["active_area"] = {
            "x_hundredths_in": 0,
            "y_hundredths_in": 0,
            "w_hundredths_in": 0,
            "h_hundredths_in": 0,
        }
    else:
        full_w_hin, full_h_hin = _effective_hin_bounds(max_x, max_y, lpi, rot)
        out["active_area"] = {
            "x_hundredths_in": max(0, int(round(xmin * full_w_hin))),
            "y_hundredths_in": max(0, int(round(ymin * full_h_hin))),
            "w_hundredths_in": max(1, int(round(max(0.05, xmax - xmin) * full_w_hin))),
            "h_hundredths_in": max(1, int(round(max(0.05, ymax - ymin) * full_h_hin))),
        }

    sa = wa.get("screen_area", {})
    sxmin = float(sa.get("x_min", 0.0))
    symin = float(sa.get("y_min", 0.0))
    sxmax = float(sa.get("x_max", 1.0))
    symax = float(sa.get("y_max", 1.0))
    if sxmin <= 0.001 and symin <= 0.001 and sxmax >= 0.999 and symax >= 0.999:
        out["screen_mapping"] = {
            "sx": 0,
            "sy": 0,
            "sw": 0,
            "sh": 0,
            "full_w": 0,
            "full_h": 0,
        }
    else:
        out["screen_mapping"] = {
            "sx": max(0, int(round(sxmin * 10000.0))),
            "sy": max(0, int(round(symin * 10000.0))),
            "sw": max(1, int(round(max(0.05, sxmax - sxmin) * 10000.0))),
            "sh": max(1, int(round(max(0.05, symax - symin) * 10000.0))),
            "full_w": 10000,
            "full_h": 10000,
        }

    pen = gui_cfg.get("pen", {})
    out["disable_pressure"] = not bool(pen.get("pressure_enabled", True))
    out["disable_tilt"] = not bool(pen.get("tilt_enabled", True))
    pc = pen.get("pressure_curve", {})
    raw_pts = pc.get("points", [])
    pts = []
    if isinstance(raw_pts, list):
        for pt in raw_pts:
            if isinstance(pt, dict) and "x" in pt and "y" in pt:
                pts.append({"x": float(pt["x"]), "y": float(pt["y"])})
    pts.sort(key=lambda pt: pt["x"])
    p1x = pts[0]["x"] if pts else float(pc.get("p1x", 0.5))
    p1y = pts[0]["y"] if pts else float(pc.get("p1y", 0.5))
    out["pressure_curve"] = {
        "p0": float(pc.get("p0", 0.0)),
        "p1x": p1x,
        "p1y": p1y,
        "p2": float(pc.get("p2", 1.0)),
        "points": pts if len(pts) >= 2 else [],
    }
    out["pen_buttons"] = {
        "barrel_1": pen_btn_to_rust_action(str(pen.get("btn1_action", "BTN_STYLUS"))),
        "barrel_2": pen_btn_to_rust_action(str(pen.get("btn2_action", "ERASER"))),
    }

    ek = gui_cfg.get("express_keys", {})
    out["disable_express_keys"] = not bool(ek.get("enabled", True))
    out["osd_notifications"] = bool(ek.get("osd_notifications", True))

    rust_ek = copy.deepcopy(out.get("express_keys", {}))
    rust_labels = copy.deepcopy(out.get("express_key_labels", {}))
    for i, k_obj in enumerate(ek.get("keys", [])):
        k_name = f"k{i + 1}"
        rust_ek[k_name] = gui_key_to_rust_action(k_obj, i)
        lbl = str(k_obj.get("label", k_name)).strip() or f"K{i + 1}"
        rust_labels[k_name] = lbl
    out["express_keys"] = rust_ek
    out["express_key_labels"] = rust_labels

    # Preenche defaults obrigatórios caso `base_rust_cfg` fosse nulo
    out.setdefault("relative_mode", False)
    out.setdefault("mouse_speed", 5)
    out.setdefault(
        "calibration",
        {"enabled": False, "f_kx": 1.0, "f_xlc": 1.0, "f_ky": 1.0, "f_ylc": 1.0},
    )
    out.setdefault("ring_1", {"cw": {"type": "key_combo", "keys": [29, 13]}, "ccw": {"type": "key_combo", "keys": [29, 12]}})
    out.setdefault("ring_2", {"cw": {"type": "mouse_wheel", "delta": 1}, "ccw": {"type": "mouse_wheel", "delta": -1}})
    out.setdefault("ring_1_modes", [])
    out.setdefault("ring_2_modes", [])
    return out
